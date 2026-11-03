import struct
for name in ['fa589d4994d61c895ab74dc5cd7db44ba1a576e13d4f8b483ba0ffb9daa6d7e3.png']:
    d = open('/root/cassette-v100/media/' + name, 'rb').read(33)
    w, h = struct.unpack('>II', d[16:24])
    print(name[:12], w, h, len(d))
