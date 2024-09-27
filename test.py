import pexpect

child = pexpect.spawn('echo Hello, World!')
child.expect(pexpect.EOF)
print(child.before.decode())
