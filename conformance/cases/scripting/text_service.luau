-- TextService beyond gui/text_bounds (docs/behavior/gui/text.md#measuring-text):
-- GetTextBoundsParams' defaults and what its properties take, what
-- GetTextBoundsAsync and GetTextSize raise for other bad arguments, bounds
-- for several built-in fonts at fixed sizes and widths, whether the two
-- agree, and that a font that can't load measures as Source Sans.
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

-- Defaults.
local p = Instance.new("GetTextBoundsParams")
log("defaults", ("%q"):format(p.Text), p.Size, p.Width, p.RichText, p.Archivable, p.Parent)
log("default font", p.Font.Family, p.Font.Weight, p.Font.Style, p.Font.Bold)
log("default font equals", p.Font == Font.new("rbxasset://fonts/families/SourceSansPro.json"))
log("same font each read", p.Font == p.Font)

-- What its properties take.
local function set(key, value)
    return caught(function()
        p[key] = value
        local v = p[key]
        return typeof(v), if typeof(v) == "Font" then v.Family else v
    end)
end
log("Size 0", set("Size", 0))
log("Size -1", set("Size", -1))
log("Size 1.5", set("Size", 1.5))
log("Size 1000", set("Size", 1000))
log("Size 1e9", set("Size", 1e9))
log("Size nan", set("Size", 0 / 0))
log("Size string number", set("Size", "12"))
log("Size string", set("Size", "x"))
log("Size nil", set("Size", nil))
log("Width -5", set("Width", -5))
log("Width 2.5", set("Width", 2.5))
log("Width inf", set("Width", math.huge))
log("Width string", set("Width", "x"))
log("Text number", set("Text", 5))
log("Text nil", set("Text", nil))
log("Text table", set("Text", {}))
log("RichText 1", set("RichText", 1))
log("RichText nil", set("RichText", nil))
log("RichText string", set("RichText", "x"))
log("Font enum", set("Font", Enum.Font.Arial))
log("Font string", set("Font", "Arial"))
log("Font nil", set("Font", nil))
log("Font fromName", set("Font", Font.fromName("Arial")))
log("unknown member", caught(function() return p.Foo end))
log("TextSize member", caught(function() return p.TextSize end))
p:Destroy()

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
log("rich bad tag", bounds(source, 20, "<b>Hello", 0, true))
log("rich escape", bounds(source, 20, "a&lt;b", 0, true), bounds(source, 20, "a<b", 0, false))
log("emoji", bounds(source, 20, "😀"))
log("unknown font", bounds(Font.new("rbxasset://fonts/families/NoSuchFont.json"), 20, fox), bounds(source, 20, fox))
log("bad font id", bounds(Font.new("rbxassetid://0"), 20, fox))

-- GetTextSize, and whether it agrees with GetTextBoundsAsync.
local function size(...)
    return caught(TextService.GetTextSize, TextService, ...)
end
local big = Vector2.new(1000, 1000)
log("GetTextSize", size(fox, 14, Enum.Font.SourceSans, big))
log("GetTextSize agrees", tostring(TextService:GetTextSize(fox, 14, Enum.Font.SourceSans, big)) == bounds(source, 14, fox))
log("GetTextSize wraps", size(fox, 18, Enum.Font.Arial, Vector2.new(100, 1000)))
log("GetTextSize wraps agrees",
    tostring(TextService:GetTextSize(fox, 18, Enum.Font.Arial, Vector2.new(100, 1000)))
        == bounds(Font.fromEnum(Enum.Font.Arial), 18, fox, 100))
log("GetTextSize short frame", size(fox, 18, Enum.Font.Arial, Vector2.new(100, 5)))
log("GetTextSize zero frame", size(fox, 18, Enum.Font.Arial, Vector2.zero))
log("GetTextSize font name", size("Hi", 14, "Arial", big))
log("GetTextSize font number", size("Hi", 14, 1, big))
log("GetTextSize bad font name", size("Hi", 14, "Nope", big))
log("GetTextSize size 0", size("Hi", 0, Enum.Font.Arial, big))
log("GetTextSize size -5", size("Hi", -5, Enum.Font.Arial, big))
log("GetTextSize size 1.5", size("Hi", 1.5, Enum.Font.Arial, big))
log("GetTextSize no frame", size("Hi", 14, Enum.Font.Arial))
log("GetTextSize frame table", size("Hi", 14, Enum.Font.Arial, {}))
log("GetTextSize no text", size(nil, 14, Enum.Font.Arial, big))
log("GetTextSize number text", size(5, 14, Enum.Font.Arial, big))
log("GetTextSize no args", size())

-- GetTextBoundsAsync's other bad arguments.
local function measure(...)
    return caught(TextService.GetTextBoundsAsync, TextService, ...)
end
log("number", measure(5))
log("table", measure({}))
log("TextLabel", measure(Instance.new("TextLabel")))
log("Font", measure(Font.fromEnum(Enum.Font.Arial)))
log("dot call", caught(function() return TextService.GetTextBoundsAsync(Instance.new("GetTextBoundsParams")) end))
do
    local q = Instance.new("GetTextBoundsParams")
    q.Text = "Hello"
    q:Destroy()
    log("destroyed params", measure(q))
    local r = Instance.new("GetTextBoundsParams")
    r.Text = "Hello"
    local copy = r:Clone()
    log("cloned params", if copy then copy.Text else "nil", if copy then measure(copy) else "")
    r:Destroy()
end
