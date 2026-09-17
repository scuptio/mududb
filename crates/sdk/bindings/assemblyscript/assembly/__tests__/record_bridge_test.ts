// Tests for the record bridge (`../record.ts`): the hand-written transcoder
// between the host's uni-data-value record-case envelope and the
// integer-keyed MessagePack map the mgen-generated record codecs consume.
//
// The Address/Profile classes below hand-mirror the mgen AssemblyScript
// record template output (`template_record_as.rs`) for this WIT shape:
//
//   record address { city: string, zip: string }
//   record profile {
//       display-name: string,   // 1
//       level: u32,             // 2  (i32 on the host wire)
//       vip: bool,              // 3  (i32 0/1 on the host wire)
//       home: option<address>,  // 4  (omitted when null: gap-fill position)
//       tags: list<string>,     // 5
//       score: f64,             // 6
//       balance: i64,           // 7
//       avatar: blob,           // 8  (record-context u8 array)
//       nickname: option<string>,// 9 (omitted when null: trailing shrink)
//   }
//
// This file is a test entry only; it is not part of the public index.ts
// exports. Compiled and driven by run_record_bridge_test.mjs, which checks
// byte-exact bridge output in both directions against an independent JS
// MessagePack encoding.

import { MpackReader, MpackWriter } from "../mpack";
import { isNullDatum, recordFieldValues, recordFromFieldValues } from "../record";
import {
  UniDataValue,
  UniDataValueArray,
  UniDataValueCodec,
  UniDataValueField,
  UniDataValueRecord,
  UniDataValueScalar,
} from "../generated/UniDataValue";
import {
  UniScalarValueBlob,
  UniScalarValueF64,
  UniScalarValueI32,
  UniScalarValueI64,
  UniScalarValueNull,
  UniScalarValueString,
} from "../generated/UniScalarValue";

// ---- mgen-template mirror: address ----

export class Address {
  city: string = "";
  zip: string = "";
}

export class AddressCodec {
  static encode(value: Address, writer: MpackWriter): void {
    writer.writeMapHeader(2);
    writer.writeU64(1);
    writer.writeString(value.city);
    writer.writeU64(2);
    writer.writeString(value.zip);
  }

  static decode(reader: MpackReader): Address {
    const value = new Address();
    const count = reader.readMapHeader();
    for (let i: u32 = 0; i < count; i++) {
      const key = reader.readMapKey();
      if (key == 1) {
        value.city = reader.readString();
      } else if (key == 2) {
        value.zip = reader.readString();
      } else {
        reader.skipValue();
      }
    }
    return value;
  }
}

// ---- mgen-template mirror: profile ----

export class Profile {
  displayName: string = "";
  level: u32 = 0;
  vip: bool = false;
  home: Address | null = null;
  tags: Array<string> = [];
  score: f64 = 0;
  balance: i64 = 0;
  avatar: Uint8Array = new Uint8Array(0);
  nickname: string | null = null;
}

export class ProfileCodec {
  static encode(value: Profile, writer: MpackWriter): void {
    // WIT option fields are omitted when null (proto3 presence semantics);
    // decode treats a missing key as the default.
    writer.writeMapHeader(9 - (value.home === null ? 1 : 0) - (value.nickname === null ? 1 : 0));
    writer.writeU64(1);
    writer.writeString(value.displayName);
    writer.writeU64(2);
    writer.writeU64(value.level as u64);
    writer.writeU64(3);
    writer.writeBool(value.vip);
    if (value.home !== null) {
      writer.writeU64(4);
      AddressCodec.encode(value.home!, writer);
    }
    writer.writeU64(5);
    writer.writeArrayHeader(value.tags.length as u32);
    for (let i = 0; i < value.tags.length; i++) {
      writer.writeString(value.tags[i]);
    }
    writer.writeU64(6);
    writer.writeF64(value.score);
    writer.writeU64(7);
    writer.writeI64(value.balance);
    writer.writeU64(8);
    writer.writeArrayHeader(value.avatar.length as u32);
    for (let i = 0; i < value.avatar.length; i++) {
      writer.writeU64(value.avatar[i]);
    }
    if (value.nickname !== null) {
      writer.writeU64(9);
      writer.writeString(value.nickname!);
    }
  }

  static decode(reader: MpackReader): Profile {
    const value = new Profile();
    const count = reader.readMapHeader();
    for (let i: u32 = 0; i < count; i++) {
      const key = reader.readMapKey();
      if (key == 1) {
        value.displayName = reader.readString();
      } else if (key == 2) {
        value.level = reader.readU64() as u32;
      } else if (key == 3) {
        // The host sends bool fields as i32 (0/1): lenient readBool.
        value.vip = reader.readBool();
      } else if (key == 4) {
        if (reader.tryNil()) {
          value.home = null;
        } else {
          value.home = AddressCodec.decode(reader);
        }
      } else if (key == 5) {
        const n = reader.readArrayHeader() as i32;
        const arr = new Array<string>(n);
        for (let i = 0; i < n; i++) {
          arr[i] = reader.readString();
        }
        value.tags = arr;
      } else if (key == 6) {
        value.score = reader.readF64();
      } else if (key == 7) {
        value.balance = reader.readI64();
      } else if (key == 8) {
        const n = reader.readArrayHeader() as i32;
        const arr = new Uint8Array(n);
        for (let i = 0; i < n; i++) {
          arr[i] = reader.readU64() as u8;
        }
        value.avatar = arr;
      } else if (key == 9) {
        if (reader.tryNil()) {
          value.nickname = null;
        } else {
          value.nickname = reader.readString();
        }
      } else {
        reader.skipValue();
      }
    }
    return value;
  }
}

// ---- envelope builders ----

function scalarI32(v: i32): UniDataValue {
  const scalar = new UniScalarValueI32();
  scalar.inner = v;
  const value = new UniDataValueScalar();
  value.inner = scalar;
  return value;
}

function scalarI64(v: i64): UniDataValue {
  const scalar = new UniScalarValueI64();
  scalar.inner = v;
  const value = new UniDataValueScalar();
  value.inner = scalar;
  return value;
}

function scalarF64(v: f64): UniDataValue {
  const scalar = new UniScalarValueF64();
  scalar.inner = v;
  const value = new UniDataValueScalar();
  value.inner = scalar;
  return value;
}

function scalarString(v: string): UniDataValue {
  const scalar = new UniScalarValueString();
  scalar.inner = v;
  const value = new UniDataValueScalar();
  value.inner = scalar;
  return value;
}

function scalarBlob(v: Uint8Array): UniDataValue {
  const scalar = new UniScalarValueBlob();
  scalar.inner = v;
  const value = new UniDataValueScalar();
  value.inner = scalar;
  return value;
}

function fieldOf(name: string, v: UniDataValue): UniDataValueField {
  const field = new UniDataValueField();
  field.field_name = name;
  field.field_value = v;
  return field;
}

function addressEnvelope(city: string, zip: string): UniDataValue {
  const record = new UniDataValueRecord();
  record.inner = [fieldOf("", scalarString(city)), fieldOf("", scalarString(zip))];
  return record;
}

function tagsEnvelope(tags: Array<string>): UniDataValue {
  const items = new Array<UniDataValue>(tags.length);
  for (let i = 0; i < tags.length; i++) {
    items[i] = scalarString(tags[i]);
  }
  const value = new UniDataValueArray();
  value.inner = items;
  return value;
}

// The host-style profile envelope: positional fields (names empty, except
// the first to prove names are ignored), bool as i32, u32 as i32, blob as
// the scalar blob case, a null option field as the Null scalar.
function profileEnvelope(withHome: bool): UniDataValue {
  const record = new UniDataValueRecord();
  const fields = new Array<UniDataValueField>(0);
  fields.push(fieldOf("display-name", scalarString("Ada")));
  fields.push(fieldOf("", scalarI32(7)));
  fields.push(fieldOf("", scalarI32(1)));
  fields.push(fieldOf("", withHome ? addressEnvelope("Paris", "75001") : nullScalarDatum()));
  fields.push(fieldOf("", tagsEnvelope(["x", "yy"])));
  fields.push(fieldOf("", scalarF64(2.5)));
  fields.push(fieldOf("", scalarI64(5000000000)));
  fields.push(fieldOf("", scalarBlob(sampleAvatarBytes())));
  fields.push(fieldOf("", scalarString("ace")));
  record.inner = fields;
  return record;
}

function nullScalarDatum(): UniDataValue {
  const scalar = new UniScalarValueNull();
  const value = new UniDataValueScalar();
  value.inner = scalar;
  return value;
}

function sampleAvatarBytes(): Uint8Array {
  const bytes = new Uint8Array(3);
  bytes[0] = 1;
  bytes[1] = 2;
  bytes[2] = 255;
  return bytes;
}

function sampleProfile(withHome: bool, withNickname: bool): Profile {
  const profile = new Profile();
  profile.displayName = "Ada";
  profile.level = 7;
  profile.vip = true;
  if (withHome) {
    const home = new Address();
    home.city = "Paris";
    home.zip = "75001";
    profile.home = home;
  }
  profile.tags = ["x", "yy"];
  profile.score = 2.5;
  profile.balance = 5000000000;
  profile.avatar = sampleAvatarBytes();
  if (withNickname) {
    profile.nickname = "ace";
  }
  return profile;
}

// ---- exports driven by run_record_bridge_test.mjs ----

// Envelope -> integer-keyed map bytes (recordFieldValues).
export function envelopeToFieldMap(withHome: bool): Uint8Array {
  return recordFieldValues(profileEnvelope(withHome));
}

// Full guest decode path: envelope bytes -> UniDataValueCodec.decode ->
// recordFieldValues -> ProfileCodec.decode -> field summary.
export function decodeProfileSummary(envelopeBytes: Uint8Array): string {
  const envelope = UniDataValueCodec.decode(new MpackReader(envelopeBytes));
  const profile = ProfileCodec.decode(new MpackReader(recordFieldValues(envelope)));
  const home = profile.home;
  const nickname = profile.nickname;
  let hex = "";
  for (let i = 0; i < profile.avatar.length; i++) {
    const b = profile.avatar[i];
    hex += (b < 16 ? "0" : "") + b.toString(16);
  }
  return (
    profile.displayName +
    "|" +
    profile.level.toString() +
    "|" +
    (profile.vip ? "true" : "false") +
    "|" +
    homeSummary(home) +
    "|" +
    profile.tags.join(",") +
    "|" +
    profile.score.toString() +
    "|" +
    profile.balance.toString() +
    "|" +
    hex +
    "|" +
    nullableSummary(nickname)
  );
}

function homeSummary(home: Address | null): string {
  if (home === null) {
    return "<null>";
  }
  return home.city + "," + home.zip;
}

function nullableSummary(s: string | null): string {
  if (s === null) {
    return "<null>";
  }
  return s;
}

// AS-side encode path: -> ProfileCodec.encode -> recordFromFieldValues ->
// UniDataValueCodec.encode -> envelope bytes.
export function envelopeFromProfile(withHome: bool, withNickname: bool): Uint8Array {
  const profile = sampleProfile(withHome, withNickname);
  const writer = new MpackWriter();
  ProfileCodec.encode(profile, writer);
  const datum = recordFromFieldValues(writer.toBytes());
  const out = new MpackWriter();
  UniDataValueCodec.encode(datum, out);
  return out.toBytes();
}

// isNullDatum recognizes exactly the Null scalar.
export function nullDatumCheck(): bool {
  const nullScalar = new UniScalarValueNull();
  const nullDatum = new UniDataValueScalar();
  nullDatum.inner = nullScalar;
  return isNullDatum(nullDatum) && !isNullDatum(scalarI32(0));
}

// recordFieldValues rejects a non-record datum (traps in wasm; the runner
// asserts the call throws).
export function rejectNonRecord(): void {
  recordFieldValues(scalarI32(1));
}
