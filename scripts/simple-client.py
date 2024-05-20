from nanomsg import Socket, PAIR, PUB


def main():
    socket = Socket(PAIR)
    socket.connect('tcp://novaview.local:2424')
    while True:
        print(socket.recv())


# main guard
if __name__ == '__main__':
    main()
