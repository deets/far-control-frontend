import time
import enum
import struct
import json
import zmq
import logging
import argparse
import serial
import itertools
import threading
import queue

logger = logging.getLogger("simple-client")


RQ_FORMAT = "<BBIhhhhhhhhhff"
RQ_SIZE = struct.calcsize(RQ_FORMAT)

RQ_STATE_FORMAT = "<BBIBBhhhh"
RQ_STATE_SIZE = struct.calcsize(RQ_STATE_FORMAT)


class PacketType(enum.Enum):
    STATE_PACKET = 0
    IMU_SET_A_PACKET = 1
    IMU_SET_B_PACKET = 2


class IgnitionState(enum.Enum):
    RESET = 0
    SECRET_A = 1
    PYROS_UNLOCKED = 2
    SECRET_AB = 3
    IGNITION = 4
    RADIO_SILENCE = 5


class ControlState(enum.Enum):
    CSM_LAUNCHPAD = 0
    CSM_IGNITION = 1
    CSM_LIFTOFF = 2
    CSM_ACCELERATION = 3
    CSM_DECELERATION = 4
    CSM_COASTNG = 5
    CSM_APOGEE = 6
    CSM_DEPLOY_MAIN = 7


def ina226_voltage(v):
    return v * 0.00125


def ina3221_voltage(v):
    return v / 8 * 0.008


class SerialSocket:

    def __init__(self, port):
        self._conn = serial.Serial(port=port, baudrate=115200)
        self._q = queue.Queue()
        t = threading.Thread(target=self._reader)
        t.daemon = True
        t.start()

    def _reader(self):
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

    def __init__(self, node, context, port=9872):
        self._clock_tracker = ClockTracker()
        self._node = node
        self._a = None
        self._b = None
        self._context = context
        self._socket = self._context.socket(zmq.PUB)
        self._socket.bind("tcp://0.0.0.0:{}".format(port))

    def feed(self, now, node, data):
        seq, packet_type = data[:2]
        packet_type = PacketType(packet_type)
        match packet_type:
            case PacketType.STATE_PACKET:
                logger.debug("STATE PACKET")
                logging.debug(data)
                values = struct.unpack(RQ_STATE_FORMAT, data[:RQ_STATE_SIZE])
                logger.info(IgnitionState(values[3]))
                logger.info(ControlState(values[4]))
                logger.info(ina226_voltage(values[5]))
                logger.info(ina3221_voltage(values[6]))
                logger.info(ina3221_voltage(values[7]))
                logger.info(ina3221_voltage(values[8]))

            case PacketType.IMU_SET_A_PACKET:
                logger.debug("IMU_SET_A_PACKET")
                values = struct.unpack(RQ_FORMAT, data[:RQ_SIZE])
                self._feed_imu_messages(now, node, *values)
            case PacketType.IMU_SET_B_PACKET:
                logger.debug("IMU_SET_B_PACKET")
                logging.debug(data[:RQ_SIZE])
                values = struct.unpack(RQ_FORMAT, data[:RQ_SIZE])
                self._feed_imu_messages(now, node, *values)

    def _feed_imu_messages(self, now, node, seq, packet_type, timestamp, acc_x, acc_y, acc_z, gyr_x, gyr_y, gyr_z, mag_x, mag_y, mag_z, pressure, temperature):
        mcu_timestamp = self._clock_tracker.feed(now, timestamp, seq)
        if node == self._node:
            data = dict(
                acc=dict(x=acc_x, y=acc_y, z=acc_z),
                gyr=dict(x=gyr_x, y=gyr_y, z=gyr_z),
                mag=dict(x=mag_x, y=mag_y, z=mag_z),
                temperature=temperature,
                pressure=pressure,
            )
            packet_type = PacketType(packet_type)
            match packet_type:
                case PacketType.IMU_SET_A_PACKET:
                    self._a = data
                case PacketType.IMU_SET_B_PACKET:
                    self._b = data
            if self._a is not None and self._b is not None:
                message = json.dumps(dict(timestamp=mcu_timestamp, raw_timestamp=timestamp, a=self._a, b=self._b))
                logging.debug(f"message: {repr(message)}")
                self._socket.send_string(message)


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--loglevel", choices=["DEBUG", "INFO", "WARNING", "ERROR"], default="INFO")
    parser.add_argument("--serial", help="If given a serial port, use that")
    parser.add_argument("--port", default="tcp://novaview.local:2424")
    parser.add_argument("--count", type=int, help="Only consume count messages. All if not given.")
    parser.add_argument("--node", default="RQB", help="Avionik node name, e.g. RQB, FDB. Default RQB")
    return parser.parse_args()


def main():
    context = zmq.Context()
    args = parse_args()
    logging.basicConfig(
        level=getattr(logging, args.loglevel)
    )
    if args.serial:
        socket = SerialSocket(args.serial)
    else:
        socket = context.socket(zmq.SUB)
        socket.setsockopt(zmq.SUBSCRIBE, b'')
        socket.connect(args.port)
    builder = MessageBuilder(args.node, context)

    for step in itertools.count():
        if args.count is not None and args.count <= step:
            break
        incoming = socket.recv()
        logging.debug("incoming: %s", incoming)
        try:
            msg = json.loads(incoming)
        except ValueError:
            logging.error("Can't parse incoming")
        else:
            now = time.monotonic()
            node, data = msg["node"], bytes(msg["data"])
            builder.feed(now, node, data)


if __name__ == '__main__':
    main()
