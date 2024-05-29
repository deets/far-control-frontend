import time
import struct
import json
import zmq

from nanomsg import Socket, PAIR, PUB


RQ_FORMAT = "<BBIhhhhhhhhhff"
RQ_SIZE = struct.calcsize(RQ_FORMAT)


class MessageBuilder:

    def __init__(self, node, port=9872):
        self._node = node
        self._a = None
        self._b = None
        self._context = zmq.Context()
        self._socket = self._context.socket(zmq.PUB)
        self._socket.bind("tcp://0.0.0.0:{}".format(port))

    def feed(self, node, seq, flags, timestamp, acc_x, acc_y, acc_z, gyr_x, gyr_y, gyr_z, mag_x, mag_y, mag_z, pressure, temperature):
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
                message = json.dumps(dict(timestamp=time.monotonic(), a=self._a, b=self._b))
                print(message)
                self._socket.send_string(message)


def main():
    socket = Socket(PAIR)
    socket.connect('tcp://novaview.local:2424')
    builder = MessageBuilder("RQB")

    while True:
        msg = socket.recv()
        print(time.monotonic(), msg)
        msg = json.loads(msg)
        node, data = msg["node"], bytes(msg["data"])
        values = struct.unpack(RQ_FORMAT, data[:RQ_SIZE])
        builder.feed(node, *values)



# main guard
if __name__ == '__main__':
    main()
