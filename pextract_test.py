import pexpect

def call_rust_analyzer(input_file):
    # Read the content of the input file
    with open(input_file, 'r') as file:
        content = file.read()

    # Start the Rust Analyzer process
    child = pexpect.spawn('rust-analyzer', encoding='utf-8', timeout=10)

    # Send the content to stdin
    child.sendline(content)

    # Wait for output
    child.expect(pexpect.EOF)

    # Get the output
    output = child.before

    # Print the output
    print("Output:")
    print(output)

if __name__ == "__main__":
    input_file = 'src/txt/initialize.txt'
    call_rust_analyzer(input_file)
