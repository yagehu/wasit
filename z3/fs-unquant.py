from random import randrange

from z3 import *

root = {
    "type": "directory",
    "opened": True,
    "opened_id": "root",
    "entries": [
        {
            "name": "f",
            "file": {
                "type": "regular-file",
            },
        },
        {
            "name": "d",
            "file": {
                "type": "directory",
                "opened": True,
                "opened_id": "root/d",
                "entries": [],
            },
        },
    ],
}

s = Solver()

def declare_option(sort: Sort):
    Option = Datatype("Option")
    Option.declare("none")
    Option.declare("some", ("inner", sort))

    return Option.create()

Fd = DeclareSort("Fd")

File = Datatype("File")
File.declare("regular_file")
File.declare("directory")
File = File.create()
OptionFile = declare_option(File)

fd_map = Function("fd_map", Fd, File, BoolSort())
fd_set = set()
file_list = list()
entries_map = Function("entries_map", File, StringSort(), File, BoolSort())

def encode_fs(root):
    dir_entry_list = []
    fd_map_list = []

    def encode_file(d):
        file = FreshConst(File)

        def encode_directory_entries(d, file):
            if d["type"] != "directory":
                return

            for entry in d["entries"]:
                child = encode_file(entry["file"])
                dir_entry_list.append((file, entry["name"], child))

        match d["type"]:
            case "regular-file":
                s.add(File.is_regular_file(file) == True)
            case "directory":
                if d["opened"]:
                    fd = Const(d["opened_id"], Fd)
                    fd_set.add(fd)
                    fd_map_list.append((fd, file))

                s.add(File.is_directory(file) == True)
            case _:
                raise f"unexpected file type {d["type"]}"

        encode_directory_entries(d, file)

        return file

    file = encode_file(root)

    clauses = []
    filename = FreshConst(StringSort())
    dir = FreshConst(File)
    child = FreshConst(File)

    for dir_, filename_, child_ in dir_entry_list:
        clauses.append(And(dir == dir_, filename == filename_, child == child_))

    s.add(ForAll(
        [dir, filename, child],
        If(
            Or(*clauses),
            entries_map(dir, filename, child),
            Not(entries_map(dir, filename, child)),
        ),
    ))

    clauses = []
    some_file = FreshConst(File)
    some_fd = FreshConst(Fd)

    # Assert Fd uniqueness
    for i, (fd, file) in enumerate(fd_map_list):
        clauses.append(And(some_file == file, some_fd == fd))

        if i > 0:
            s.add(fd_map_list[i - 1][0] != fd)

    s.add(ForAll(
        [some_file, some_fd],
        If(
            Or(*clauses),
            fd_map(some_fd, some_file),
            Not(fd_map(some_fd, some_file)),
        )
    ))

    file_list.append(file)

    return file


root = encode_fs(root)


n = randrange(1, 10)
n = 10

print(f"path segments: {n}")

Segment = Datatype("Segment")
Segment.declare("separator")
Segment.declare("component", ("string", StringSort()))
Segment = Segment.create()

segments = []
component_idx_accs = []

for i in range(n):
    segment = Const(f"param-path-{i}", Segment)
    segments.append(segment)
    component_idx_accs.append(FreshInt())

component_idx = Function("component_idx", Segment, IntSort())

for i in range(len(segments)):
    segment = segments[i]
    s.add(Implies(
        Segment.is_component(segment),
        And(
            Not(Contains(Segment.string(segment), StringVal("/"))),
            Length(Segment.string(segment)) > 0,
        ),
    ))

    if i == 0:
        s.add(Segment.is_component(segment))

    if i > 0:
        nei = segments[i - 1]

        s.add(Implies(
            Segment.is_component(segment),
            Segment.is_separator(nei),
        ))

    component_idx_accs[i] = FreshInt()
    s.add(If(
        Segment.is_component(segment),
        component_idx_accs[i] == 1,
        component_idx_accs[i] == 0,
    ))
    s.add(If(
        Segment.is_component(segment),
        component_idx(segment) == sum(component_idx_accs[:i]),
        component_idx(segment) == -1,
    ))

def at_least_components(n: int):
    if n == 0:
        return

    is_component = [None] * len(segments)

    for i, segment in enumerate(segments):
        is_component[i] = FreshInt()
        s.add(If(
            Segment.is_component(segment),
            is_component[i] == 1,
            is_component[i] == 0,
        ))

    s.add(sum(is_component) >= n)


param_fd = Const("param_fd", Fd)

# Ensure the fd param is chosen from the set of declared fds.
s.add(Or(*[param_fd == fd for fd in fd_set]))

# Make sure there are multiple components just to make paths more interesting.
at_least_components(3)


component_file_map = Function("component_file_map", Segment, File, BoolSort())
component_file_list = list()
curr_file = FreshConst(File)
s.add(Or(*[curr_file == file for file in file_list]))
for i in range(len(segments) - 1):
    seg = segments[i]
    component = FreshConst(Segment)
    next_component = FreshConst(Segment)
    some_file = FreshConst(File)

    s.add(Or(*[component == segment for segment in segments]))
    s.add(Or(*[next_component == segment for segment in segments]))
    s.add(Or(
        component_idx(next_component) == i + 1,
        component_idx(next_component) == 0,
    ))

    if i == 0:
        root_file = FreshConst(OptionFile)

        # Start resolving each component from param_fd.
        s.add(fd_map(param_fd, root_file))

        component_file_list.append(root_file)


def test_unsat():
    s.push()
    some_file = FreshConst(File)
    component_0 = FreshConst(Segment)
    component_1 = FreshConst(Segment)
    s.add(param_fd == Const("root", Fd))
    s.add(And(
        Segment.is_component(component_0),
        component_idx(component_0) == 0,
    ))
    s.add(And(
        Segment.is_component(component_1),
        component_idx(component_1) == 1,
    ))
    s.add(Or(*[segment == component_0 for segment in segments]))
    s.add(Or(*[segment == component_1 for segment in segments]))
    s.add(ForAll(
        [some_file],
        Not(entries_map(root, Segment.string(component_0), some_file))
    ))
    s.add(Segment.string(component_1) == "..")
    assert s.check() == sat, f"{ s.check() }"
    m = s.model()
    for segment in segments:
        print(m.evaluate(segment))
    s.pop()

test_unsat()

exit(1)
result = s.check()

assert result == sat, f"{result}"

m = s.model()

param_path = ""

# Render the path
for segment in segments:
    if m.evaluate(Segment.is_separator(segment)).__bool__():
        param_path += "/"
    else:
        component = m.evaluate(Segment.string(segment)).as_string()
        idx = m.evaluate(component_idx(segment)).as_long()
        param_path += component
        print(f"{component} -> {idx}")

print(m)
print("path chosen:", param_path)

for fd in fd_set:
    if m.evaluate(fd) == m[param_fd]:
        print("  fd chosen:", fd)

