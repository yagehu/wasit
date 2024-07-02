from z3 import *

base_dir = {
    "f": {
        "type": "regular-file",
    },
    "d": {
        "type": "directory",
        "entries": {
            "d0f": {
                "type": "regular-file",
            },
            "d1f": {
                "type": "regular-file",
            },
            "d2d": {
                "type": "directory",
            },
        }
    },
}

s = Solver()

def declare_option(sort: Sort):
    Option = Datatype("Option")
    Option.declare("none")
    Option.declare("some", ("inner", sort))

    return Option.create()

File = Datatype("File")
File.declare("regular-file")
File.declare("directory")
File = File.create()
OptionFile = declare_option(File)

Segment = Datatype("Segment")
Segment.declare("separator")
Segment.declare("component", ("string", StringSort()))
Segment = Segment.create()

Path = Datatype("Path")
Path.declare("path", ("segments", SeqSort(Segment)))
Path = Path.create()

Fd = DeclareSort("Fd")

base_fd = Const("base-fd", Fd)
fd1 = Const("fd1", Fd)

fd_pool = [
    {
        "name": "base-direcotry",
        "value": base_fd,
    },
    {
        "name": "fd1",
        "value": fd1,
    },
]

param_fd = Const("param-fd", Fd)
param_path = Const("param-path", Path)

path_idx = Int("path-idx")

s.add(And(
    base_fd != fd1,
))
s.add(
    Or(
        param_fd == base_fd,
        param_fd == fd1,
    )
)

def path_idx_in_bound(path):
    return And(0 <= path_idx, path_idx < Length(Path.segments(path)))

s.add(ForAll(
    [path_idx],
    Implies(
        Not(path_idx_in_bound(param_path)),
        Path.segments(param_path)[path_idx] == Segment.component(StringVal("")),
    ),
))
# Path must be non-empty.
s.add(Length(Path.segments(param_path)) > 0)
s.add(Length(Path.segments(param_path)) < 8)
# Path must not be absolute.
s.add(Not(Segment.is_separator(Path.segments(param_path)[0])))
# Path components must not be empty and must not contain separators "/".
s.add(
    ForAll(
        [path_idx],
        Implies(
            And(
                path_idx_in_bound(param_path),
                Segment.is_component(Path.segments(param_path)[path_idx]),
            ),
            And(
                Length(Segment.string(Path.segments(param_path)[path_idx])) > 0,
                Length(Segment.string(Path.segments(param_path)[path_idx])) <= 2,
                Not(Contains(Segment.string(Path.segments(param_path)[path_idx]), "/")),
            ),
        ),
    ),
)
# Adjacent path segments can't be both components.
s.add(
    ForAll(
        [path_idx],
        Implies(
            And(
                path_idx_in_bound(param_path),
                path_idx < Length(Path.segments(param_path)) - 1,
            ),
            Not(And(
                Segment.is_component(Path.segments(param_path)[path_idx]),
                Segment.is_component(Path.segments(param_path)[path_idx + 1]),
            )),
        ),
    ),
)

component_idx = Function("component-idx", Path, IntSort(), IntSort())
idx = Int("idx")
acc = Int("component-idx--acc")
seg = Const("component-idx--seg", Segment)
path = Const("path", Path)
s.add(ForAll(
    [path, path_idx],
    component_idx(path, path_idx) == If(
        And(
            path_idx_in_bound(path),
            Segment.is_component(Path.segments(path)[path_idx])
        ),
        SeqFoldLeftI(
            Lambda([idx, acc, seg], If(
                And(Segment.is_component(seg), idx <= path_idx),
                acc + 1,
                acc,
            )),
            0,
            0,
            Path.segments(path),
        ) - 1,
        -1,
    )
))


component_to_file = Function("component-to-file", Path, IntSort(), OptionFile)
# curr_fd = Const("curr-fd", Fd)
s.add(ForAll(
    [path_idx],
    If(
        path_idx_in_bound(param_path),
        component_to_file(param_path, path_idx) == OptionFile.none,
        component_to_file(param_path, path_idx) == OptionFile.none,
    ),
))


def unsat_cases():
    def absolute_path():
        s.add(Path.segments(param_path)[0] == Segment.separator)
    
    def contiguous_path_components():
        s.add(And(
            Length(Path.segments(param_path)) >= 2,
            Path.segments(param_path)[0] == Segment.component(StringVal("a")),
            Path.segments(param_path)[1] == Segment.component(StringVal("d")),
        ))

    cases = [
        absolute_path,
        contiguous_path_components,
    ]

    for i, c in enumerate(cases):
        print(f"Negative test case {i}")
        s.push()
        c()
        result = s.check()
        if result == sat:
            print(s.model())
        assert result == unsat, f"Expected unsat, got {result}"
        s.pop()

def sat_cases():
    def component_idx_ok():
        s.add(Length(Path.segments(param_path)) == 3)
        s.add(Path.segments(param_path)[0] == Segment.component(StringVal("a")))
        s.add(Path.segments(param_path)[1] == Segment.separator)
        s.add(Path.segments(param_path)[2] == Segment.component(StringVal("b")))
        s.add(component_idx(param_path, 0) == 0)
        s.add(component_idx(param_path, 1) == -1)
        s.add(component_idx(param_path, 2) == 1)

    cases = [
        component_idx_ok,
    ]

    for i, c in enumerate(cases):
        print(f"Positive test case {i}")
        s.push()
        c()
        result = s.check()
        assert result == sat, f"Expected sat, got {result}"
        s.pop()


unsat_cases()
sat_cases()


s.add(Length(Path.segments(param_path)) == 7)


result = s.check()

assert result == sat, f"{result}"
m = s.model()

p = ""

for i in range(0, m.eval(Length(Path.segments(m[param_path]))).as_long()):
    segment = m.eval(Path.segments(m[param_path])[i])

    if m.eval(Segment.is_separator(segment)).__bool__():
        p += "/"
    else:
        p += m.eval(Segment.string(segment)).as_string()

print("path", p)

for fd_entry in fd_pool:
    if m[param_fd] == m.eval(fd_entry["value"]):
        print(f"chosen fd: {fd_entry["name"]}")
        break

print(m[component_to_file])