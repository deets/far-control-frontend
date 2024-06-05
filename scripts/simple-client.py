import time
import struct
import json
import zmq
import logging
import argparse
import serial
import threading
import queue

from nanomsg import Socket, PAIR, PUB

logger = logging.getLogger(__name__)


RQ_FORMAT = "<BBIhhhhhhhhhff"
RQ_SIZE = struct.calcsize(RQ_FORMAT)


class SerialSocket:

    def __init__(self, port):
        self._conn = serial.Serial(port=port, baudrate=115200)
        self._q = queue.Queue()
        t = threading.Thread(target=self._reader)
        t.daemon = True
        t.start()

    def _reader(self):
        buffer = b""
        while True:
            line = self._conn.readline()
            if line.startswith(b"###"):
                data = list(bytes.fromhex(line[3:-2].decode("ascii")))
                j = json.dumps(dict(node="RQB", data=data))
                self._q.put(j)

    def recv(self):
        return self._q.get()


class ClockTracker:

    def __init__(self, clockfreq=200_000_000):
        self._clockfreq = clockfreq
        self._last_timestamp = None
        self._last_host = None
        self._last_seq = None
        self._mcu_time = time.monotonic()

    def feed(self, now, timestamp, seq):
        if self._last_timestamp is not None:
            seq_diff = (seq + 2**8 - self._last_seq) % 2**8
            diff = (timestamp + 2**32 - self._last_timestamp) % 2**32  / seq_diff / self._clockfreq
            self._mcu_time += diff

        self._last_timestamp = timestamp
        self._last_host = now
        self._last_seq = seq
        return self._mcu_time


class MessageBuilder:

    def __init__(self, node, port=9872):
        self._clock_tracker = ClockTracker()
        self._node = node
        self._a = None
        self._b = None
        self._context = zmq.Context()
        self._socket = self._context.socket(zmq.PUB)
        self._socket.bind("tcp://0.0.0.0:{}".format(port))

    def feed(self, now, node, data):
        seq, flags_and_message_type = data[:2]
        if flags_and_message_type & 0x0f == 0:
            values = struct.unpack(RQ_FORMAT, data[:RQ_SIZE])
            self._feed_imu_messages(now, node, *values)

    def _feed_imu_messages(self, now, node, seq, flags, timestamp, acc_x, acc_y, acc_z, gyr_x, gyr_y, gyr_z, mag_x, mag_y, mag_z, pressure, temperature):
        mcu_timestamp = self._clock_tracker.feed(now, timestamp, seq)

        if node == self._node:
            data = dict(
                acc=dict(x=acc_x, y=acc_y, z=acc_z),
                gyr=dict(x=gyr_x, y=gyr_y, z=gyr_z),
                mag=dict(x=mag_x, y=mag_y, z=mag_z),
                temperature=temperature,
                pressure=pressure,
            )
            if flags & 0x80:
                self._b = data
            else:
                self._a = data
            if self._a is not None and self._b is not None:
                message = json.dumps(dict(timestamp=mcu_timestamp, raw_timestamp=timestamp, a=self._a, b=self._b))
                logging.debug(f"message: {repr(message)}")
                self._socket.send_string(message)


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--loglevel", choices=["DEBUG", "INFO", "WARNING", "ERROR"], default="INFO")
    parser.add_argument("--serial", help="If given a serial port, use that")
    return parser.parse_args()


def main():
    args = parse_args()
    logging.basicConfig(
        level=getattr(logging, args.loglevel)
    )
    if args.serial:
        socket = SerialSocket(args.serial)
    else:
        socket = Socket(PAIR)
        socket.connect('tcp://novaview.local:2424')
    builder = MessageBuilder("RQB")

    while True:
        msg = json.loads(socket.recv())
        now = time.monotonic()
        node, data = msg["node"], bytes(msg["data"])
        builder.feed(now, node, data)



# main guard
if __name__ == '__main__':
    main()
