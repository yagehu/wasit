abstract sig Input {
  input: set Type,
}

one sig Rng extends Input {}

lone sig BaseDir extends Input {
} {
  input = Fd
}

abstract sig Type {}

abstract sig ValType extends Type {}
abstract sig TypeDef extends Type {
  type: one ValType,
}

abstract sig Fundamental extends ValType {}
abstract sig Numerical extends Fundamental {}
abstract sig Container extends Fundamental {}
abstract sig Specialized extends ValType {}
abstract sig Handle extends Fundamental {}

fun type_of: ValType -> TypeDef { ~type }

sig Resource in TypeDef {
  fulfills: lone Resource,
  sizes: lone Resource,
}

fun includes: Resource -> Resource { ~fulfills }

abstract sig Function {
  param: set TypeDef,
  result: set TypeDef,
}

fun param_of: TypeDef -> Function { ~param }

lone sig
    I32
  , U8
  , U32
  , U64
  extends Numerical {}

sig List extends Container {
  element: one Type,
}

fun element_of: Type -> List { ~element }

sig Flags extends Specialized {}

sig Record extends Container {
  member: set Type,
}

fun member_of: Type -> Record { ~member }

fun structural: Type -> Container {
  member_of + element_of
}

sig Str extends Specialized {}
sig Variant extends Specialized {}

fact no_dangling_valtype {
  all t: ValType | some t.*dep.param_of or some t.*dep.~result
}

one sig
    Advice
  , Fd
  , Fdflags
  , FileOffset
  , FileSliceLen
  , Iovec
  , IovecArray
  , Lookupflags
  , NByte
  , Oflags
  , Path
  , Rights
  extends TypeDef {}

fact resources {
  Fd in Resource
  no Fd.fulfills

  FileOffset in Resource
  no FileOffset.fulfills

  NByte in Resource
  NByte.fulfills = FileSliceLen
}

one sig
    FdAdvise
  , FdRead
  , FdTell
  , PathOpen
  extends Function {}

fact {
  Advice.type = Variant

  Fd.type = Handle

  Fdflags.type = Flags

  FileOffset.type = U64

  FileSliceLen.type = U64

  Iovec.type in List
  Iovec.type.element = U8

  IovecArray.type in List
  IovecArray.type.element = Iovec

  Lookupflags.type = Flags

  NByte.type = U32

  Oflags.type = Flags

  Path.type = Str

  Rights.type = Flags
}

fact fd_advise {
  FdAdvise.param = Fd + FileOffset + FileSliceLen + Advice
  no FdAdvise.result
}

fact fd_read {
  FdRead.param = Fd + IovecArray
  FdRead.result = NByte
}

fact fd_tell {
  FdTell.param = Fd
  FdTell.result = FileOffset
}

fact path_open {
  PathOpen.param = Fd + Lookupflags + Path + Oflags + Rights + Fdflags
  PathOpen.result = Fd
}

pred generate_flags { Flags in Rng.input }
pred no_generate_flags { Flags not in Rng.input }

pred generate_numerical_types {
  all n: Numerical | n in Rng.input
}

pred mount_base_dir { one BaseDir }

fun dep: univ -> univ {
  input + structural + type_of
}

fun bi_resources[r: Resource]: set Resource {
  r.*fulfills.*includes
}

check funcs_reachable {
  {
    generate_flags
    generate_numerical_types
    mount_base_dir
  } =>

  all f: Function {
    all p: f.param {
      some (p.*(~dep) + bi_resources[p].^(~dep)) & Input
    }
  }
} for 30

run {
  generate_flags
  generate_numerical_types
  mount_base_dir
} for 30
