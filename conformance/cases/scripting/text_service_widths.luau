-- TextService's bounds, which depend on each font's glyph widths: 18
-- built-in fonts at fixed sizes and widths, spaces, line breaks, sizes,
-- weights, rich text, an emoji, a font that can't load, GetTextSize, and a
-- destroyed GetTextBoundsParams (docs/behavior/gui/text.md#measuring-text).
-- Split from scripting/text_service, which holds what doesn't depend on
-- glyph widths. It has no expectation file: Rook's glyph advances, emoji
-- fallback, carriage return and GetTextSize at size 0 don't match Studio
-- yet (docs/behavior/gui/text.md#not-yet). What Studio 0.748.0.7481526
-- printed, with deferred signals, line for line:
--
-- SourceSans 216, 14 58, 32 94, 72
-- SourceSansBold 231, 14 63, 32 81, 72
-- Arial 260, 14 65, 32 89, 72
-- ArialBold 281, 14 71, 32 96, 72
-- Gotham 268, 14 69, 32 95, 72
-- GothamBold 288, 14 73, 32 100, 72
-- Roboto 248, 14 65, 32 87, 72
-- RobotoMono 301, 14 75, 32 90, 90
-- Code 301, 14 80, 32 90, 90
-- Ubuntu 260, 14 69, 32 93, 72
-- Montserrat 268, 14 69, 32 95, 72
-- Oswald 155, 14 40, 32 94, 54
-- Legacy 384, 21 97, 48 92, 189
-- Cartoon 256, 14 63, 32 90, 72
-- Fantasy 255, 14 68, 32 85, 72
-- Highway 237, 14 59, 32 83, 72
-- SciFi 264, 14 61, 32 93, 72
-- Arcade 344, 14 90, 32 100, 90
-- one letter 13, 20 4, 20 4, 20
-- spaces only 3, 20 9, 20
-- trailing spaces 17, 20 26, 20
-- leading spaces 26, 20
-- tab 29, 20
-- newlines 8, 40 0, 40 9, 60
-- crlf 11, 40
-- size 1 3, 1
-- size 100 176, 100
-- size 101 176, 100
-- size 0 3, 1
-- fractional size 36, 20
-- width exact 40, 40
-- long word in width 39, 80
-- width negative 79, 20
-- weights 327, 20 297, 20
-- italic 298, 20
-- rich bold 39, 20
-- rich line break 9, 40
-- rich escape 25, 20 25, 20
-- emoji 18, 20
-- unknown font error Temp read failed. 304, 20
-- GetTextSize ok 216, 14
-- GetTextSize wraps ok 89, 72
-- GetTextSize short frame ok 89, 18
-- GetTextSize zero frame ok 328, 18
-- GetTextSize font name ok 12, 14
-- GetTextSize font number ok 12, 14
-- GetTextSize size 0 ok 1, 0
-- GetTextSize size -5 ok 1, 0
-- GetTextSize size 1.5 ok 3, 2
-- GetTextSize number text ok 7, 14
-- destroyed params ok 36, 20

local TextService = game:GetService("TextService")

local function caught(f, ...)
    local r = table.pack(pcall(f, ...))
    if r[1] then
        local s = {}
        for i = 2, r.n do
            s[#s + 1] = tostring(r[i])
        end
        return "ok", table.concat(s, " ")
    end
    local e = r[2]
    return "error", type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end

-- Bounds.
local function bounds(font, size, text, width, rich)
    local q = Instance.new("GetTextBoundsParams")
    q.Font = font
    q.Size = size
    q.Text = text
    q.Width = width or 0
    q.RichText = rich or false
    local ok, b = pcall(TextService.GetTextBoundsAsync, TextService, q)
    q:Destroy()
    if not ok then
        return "error " .. tostring(b)
    end
    return tostring(b)
end
local fox = "The quick brown fox jumps over the lazy dog"
local fonts = {
    "SourceSans",
    "SourceSansBold",
    "Arial",
    "ArialBold",
    "Gotham",
    "GothamBold",
    "Roboto",
    "RobotoMono",
    "Code",
    "Ubuntu",
    "Montserrat",
    "Oswald",
    "Legacy",
    "Cartoon",
    "Fantasy",
    "Highway",
    "SciFi",
    "Arcade",
}
for _, name in fonts do
    local f = Font.fromEnum(Enum.Font[name])
    log(name, bounds(f, 14, fox), bounds(f, 32, "Hello"), bounds(f, 18, fox, 100))
end

local source = Font.fromEnum(Enum.Font.SourceSans)
log("one letter", bounds(source, 20, "W"), bounds(source, 20, "i"), bounds(source, 20, "."))
log("spaces only", bounds(source, 20, " "), bounds(source, 20, "   "))
log("trailing spaces", bounds(source, 20, "ab"), bounds(source, 20, "ab   "))
log("leading spaces", bounds(source, 20, "   ab"))
log("tab", bounds(source, 20, "a\tb"))
log("newlines", bounds(source, 20, "a\n"), bounds(source, 20, "\n"), bounds(source, 20, "a\n\nb"))
log("crlf", bounds(source, 20, "a\r\nb"))
log("size 1", bounds(source, 1, "Hello"))
log("size 100", bounds(source, 100, "Hello"))
log("size 101", bounds(source, 101, "Hello"))
log("size 0", bounds(source, 0, "Hello"))
log("fractional size", bounds(source, 20.5, "Hello"))
log("width exact", bounds(source, 20, "Hello world", 50))
log("long word in width", bounds(source, 20, "Supercalifragilistic", 40))
log("width negative", bounds(source, 20, "Hello world", -10))
log("weights", bounds(Font.new(source.Family, Enum.FontWeight.Bold), 20, fox),
    bounds(Font.new(source.Family, Enum.FontWeight.Light), 20, fox))
log("italic", bounds(Font.new(source.Family, Enum.FontWeight.Regular, Enum.FontStyle.Italic), 20, fox))
log("rich bold", bounds(source, 20, "<b>Hello</b>", 0, true))
log("rich line break", bounds(source, 20, "a<br/>b", 0, true))
log("rich escape", bounds(source, 20, "a&lt;b", 0, true), bounds(source, 20, "a<b", 0, false))
log("emoji", bounds(source, 20, "😀"))
log("unknown font", bounds(Font.new("rbxasset://fonts/families/NoSuchFont.json"), 20, fox), bounds(source, 20, fox))

-- GetTextSize.
local function size(...)
    return caught(TextService.GetTextSize, TextService, ...)
end
local big = Vector2.new(1000, 1000)
log("GetTextSize", size(fox, 14, Enum.Font.SourceSans, big))
log("GetTextSize wraps", size(fox, 18, Enum.Font.Arial, Vector2.new(100, 1000)))
log("GetTextSize short frame", size(fox, 18, Enum.Font.Arial, Vector2.new(100, 5)))
log("GetTextSize zero frame", size(fox, 18, Enum.Font.Arial, Vector2.zero))
log("GetTextSize font name", size("Hi", 14, "Arial", big))
log("GetTextSize font number", size("Hi", 14, 1, big))
log("GetTextSize size 0", size("Hi", 0, Enum.Font.Arial, big))
log("GetTextSize size -5", size("Hi", -5, Enum.Font.Arial, big))
log("GetTextSize size 1.5", size("Hi", 1.5, Enum.Font.Arial, big))
log("GetTextSize number text", size(5, 14, Enum.Font.Arial, big))

-- A destroyed GetTextBoundsParams still measures.
do
    local q = Instance.new("GetTextBoundsParams")
    q.Text = "Hello"
    q:Destroy()
    log("destroyed params", caught(TextService.GetTextBoundsAsync, TextService, q))
end
