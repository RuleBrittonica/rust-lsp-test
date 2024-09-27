import wexpect

# Start a child process running the command `echo Hello, World!`
child = wexpect.spawn('echo Hello, World!')

# Wait for the process to finish and reach the end of output (EOF)
child.expect(wexpect.EOF)

# Output up to the EOF is captured in child.before, which is then decoded from bytes to a string
print(child.before.decode())
