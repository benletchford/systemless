"""Patch only compiled DLOG131 procID4 to5; never overwrite the input fork."""
import argparse
import struct
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('resource_fork', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
assert args.resource_fork.resolve() != args.output.resolve()
original = args.resource_fork.read_bytes()
resource = bytearray(original)
data, map_offset, data_length, map_length = struct.unpack_from('>IIII', resource)
assert data + data_length <= len(resource)
assert map_offset + map_length <= len(resource)
types = map_offset + struct.unpack_from('>H', resource, map_offset + 24)[0]
count = struct.unpack_from('>H', resource, types)[0] + 1
matches = []
for index in range(count):
    entry = types + 2 + index * 8
    if resource[entry:entry + 4] != b'DLOG':
        continue
    last, references = struct.unpack_from('>HH', resource, entry + 4)
    for item in range(last + 1):
        reference = types + references + item * 12
        if struct.unpack_from('>h', resource, reference)[0] != 131:
            continue
        record = data + int.from_bytes(resource[reference + 5:reference + 8], 'big')
        length = struct.unpack_from('>I', resource, record)[0]
        assert length >= 18 and record + 4 + length <= data + data_length
        proc_offset = record + 12
        assert struct.unpack_from('>h', resource, proc_offset)[0] == 4
        matches.append(proc_offset)
assert len(matches) == 1
struct.pack_into('>h', resource, matches[0], 5)
assert sum(a != b for a, b in zip(original, resource)) == 1
args.output.write_bytes(resource)
