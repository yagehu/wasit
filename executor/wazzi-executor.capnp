@0xd4cc3e39bec805f1;

enum Func {
  argsGet              @0;
  argsSizesGet         @1;
  environGet           @2;
  environSizesGet      @3;
  clockResGet          @4;
  clockTimeGet         @5;
  fdAdvise             @6;
  fdAllocate           @7;
  fdClose              @8;
  fdDatasync           @9;
  fdFdstatGet          @10;
  fdFdstatSetFlags     @11;
  fdFdstatSetRights    @12;
  fdFilestatGet        @13;
  fdFilestatSetSize    @14;
  fdFilestatSetTimes   @15;
  fdPread              @16;
  fdPrestatGet         @17;
  fdPrestatDirName     @18;
  fdPwrite             @19;
  fdRead               @20;
  fdReaddir            @21;
  fdRenumber           @22;
  fdSeek               @23;
  fdSync               @24;
  fdTell               @25;
  fdWrite              @26;
  pathCreateDirectory  @27;
  pathFilestatGet      @28;
  pathFilestatSetTimes @29;
  pathLink             @30;
  pathOpen             @31;
  pathReadlink         @32;
  pathRemoveDirectory  @33;
  pathRename           @34;
  pathSymlink          @35;
  pathUnlinkFile       @36;
  pollOneoff           @37;
  procExit             @38;
  procRaise            @39;
  schedYield           @40;
  randomGet            @41;
  sockAccept           @42;
  sockRecv             @43;
  sockSend             @44;
  sockShutdown         @45;
}

struct Request {
  union {
    decl @0 :DeclRequest;
    call @1 :CallRequest;
  }
}

struct Response {
  union {
    decl @0 :DeclResponse;
    call @1 :CallResponse;
  }
}

struct DeclRequest {
  resourceId @0 :UInt64;
  value      @1 :Value;
}

struct DeclResponse {
}

struct CallRequest {
  func    @0 :Func;
  params  @1 :List(ParamSpec);
  results @2 :List(ResultSpec);
}

struct CallResponse {
  return  @0 :CallReturn;
  results @1 :List(Result);
}

struct CallReturn {
  union {
    none  @0 :Void;
    errno @1 :Int32;
  }
}

struct ParamSpec {
  type @0 :Type;

  union {
    resource @1 :ResourceRef;
    value    @2 :Value;
  }
}

struct ResourceRef {
  id @0 :UInt64;
}

struct ResultSpec {
  union {
    ignore   @0 :Void;
    resource @1 :Void;
  }
}

struct Result {
  memoryOffset @0 :UInt32;
}

struct Type {
  enum IntRepr {
    u8  @0;
    u16 @1;
    u32 @2;
    u64 @3;
  }

  struct Builtin {
    union {
      u8   @0 :Void;
      u16  @1 :Void;
      u32  @2 :Void;
      u64  @3 :Void;
      s8   @4 :Void;
      s16  @5 :Void;
      s32  @6 :Void;
      s64  @7 :Void;
      char @8 :Void;
    }
  }

  struct Bitflags {
    struct Member {
      name @0 :Text;
    }

    members @0 :List(Member);
    repr    @1 :IntRepr;
  }

  struct Record {
    struct Member {
      name   @0 :Text;
      type   @1 :Type;
      offset @2 :UInt32;
    }

    members @0 :List(Member);
    size    @1 :UInt32;
  }

  struct Array {
    item     @0 :Type;
    itemSize @1 :UInt32;
  }

  struct Variant {
    struct Case {
      name @0 :Text;
      type @1 :Type;
    }

    tagRepr       @0 :IntRepr;
    cases         @1 :List(Case);
    payloadOffset @2 :UInt32;
    size          @3 :UInt32;
  }

  union {
    # builtin     @0 :Builtin;
    # record      @1 :Record;
    # array       @2 :Array;
    # variant     @4 :Variant;
    # allocBuffer @5 :Void;
    bool     @0 :Void;
    string   @1 :Void;
    bitflags @2 :Bitflags;
    handle   @3 :Void;
  }
}

struct Value {
  struct Bitflags {
    members @0 :List(Bool);
  }

  union {
    bool     @0 :Bool;
    string   @1 :Text;
    bitflags @2 :Bitflags;
    handle   @3 :UInt32;
  }
}
