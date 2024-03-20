abstract sig ValueType {}

abstract sig ValueTypeFundamental extends ValueType {}
abstract sig ValueTypeSpecialized extends ValueType {}
abstract sig ValueTypeUserDefined extends ValueType {
  parent: one ValueType,
}

abstract sig Function {
  params: set ValueType,
  results: set ValueType,
}

fun parent_of: ValueType -> ValueTypeUserDefined { ~parent }
fun param_of: ValueType -> Function { ~params }

one sig
    I32
  , U8
  , U32
  , U64
  extends ValueTypeFundamental {}

sig List extends ValueTypeSpecialized {
  element: one ValueType,
}

fun element_of: ValueType -> List { ~element }

sig Flags extends ValueTypeSpecialized {}

sig Pointer extends ValueTypeSpecialized {
  pointee: one ValueType,
}

fun pointee_of: ValueType -> Pointer { ~pointee }

sig Record extends ValueTypeSpecialized {
  members: set ValueType,
}

fun member_of: ValueType -> Record { ~members }

sig Str extends ValueTypeSpecialized {}
sig Variant extends ValueTypeSpecialized {}

one sig
    Advice
  , Fd
  , FdBase
  , FdDir
  , FdReg
  , Fdflags
  , FileOffset
  , FileSliceLen
  , Iovec
  , IovecArray
  , Lookupflags
  , NByte
  , Oflags
  , Path
  , ReadBuf
  , ReadBufLen
  , Rights
  extends ValueTypeUserDefined {}

one sig
    FdAdvise
  , FdRead
  , FdTell
  , PathOpen
  extends Function {}

fact {
  all t: ValueTypeSpecialized | some t.^parent_of
}

fact {
  Advice.parent = Variant

  Fd.parent = I32

  FdDir.parent = Fd

  FdReg.parent = Fd

  FdBase.parent = FdDir

  Fdflags.parent = Flags

  FileOffset.parent = U64

  FileSliceLen.parent = U64

  Iovec.parent = Record
  Iovec.parent.members = ReadBuf + ReadBufLen
  (Iovec.parent.members & ReadBuf).parent.pointee = U8

  IovecArray.parent in List
  IovecArray.parent.element = Iovec

  Lookupflags.parent = Flags

  NByte.parent = U32

  Oflags.parent = Flags

  Path.parent = Str

  ReadBuf.parent in Pointer

  ReadBufLen.parent = U32

  Rights.parent = Flags
}

fact fd_advise {
  FdAdvise.params = FdReg + FileOffset + FileSliceLen + Advice
  no FdAdvise.results
}

fact fd_read {
  FdRead.params = FdReg + IovecArray
  FdRead.results = NByte
}

fact fd_tell {
  FdTell.params = FdReg
  FdTell.results = FileOffset
}

fact path_open {
  PathOpen.params = FdDir + Lookupflags + Path + Oflags + Rights + Fdflags
  PathOpen.results = Fd
}

run {} for 100
