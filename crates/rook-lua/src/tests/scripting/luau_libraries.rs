//! [Luau's libraries](docs/behavior/scripting/luau-libraries.md): what the
//! Luau Roblox runs gives scripts besides Roblox's own API.

use crate::tests::*;

#[test]
fn modern_luau_libraries_are_there() {
    expect(
        r##"
        local b = buffer.create(8)
        buffer.writeu32(b, 0, 0xDEADBEEF)
        buffer.writef32(b, 4, 1.5)
        print(typeof(b), buffer.len(b), string.format("%x", buffer.readu32(b, 0)), buffer.readf32(b, 4))
        print(buffer.tostring(buffer.fromstring("hi")), buffer.readstring(buffer.fromstring("hello"), 1, 3))
        print(typeof(vector.create(1, 2, 3)), vector.magnitude(vector.create(3, 4, 0)), vector.create(1, 2, 3) == Vector3.new(1, 2, 3))
        print(bit32.band(0xFF, 0x0F), utf8.char(72, 105), table.isfrozen(table.freeze({})))
        print(#string.pack("i4", 1), math.clamp(5, 0, 3), math.sign(-2), math.round(2.5))
        print(table.clone({ 1, 2 })[2], table.find({ "a", "b" }, "b"), select("#", table.unpack({ 1, 2, 3 })))
        print(coroutine.close(coroutine.create(function() end)), math.lerp(0, 10, 0.25), math.map(5, 0, 10, 0, 100))
        print(`{1 + 1} {"interpolated"}`, if true then "if-expression" else "no")
        "##,
        0,
        &[
            "buffer 8 deadbeef 1.5",
            "hi ell",
            "Vector3 5 true",
            "15 Hi true",
            "4 3 -1 3",
            "2 2 3",
            "true 2.5 50",
            "2 interpolated if-expression",
        ],
    );
}
