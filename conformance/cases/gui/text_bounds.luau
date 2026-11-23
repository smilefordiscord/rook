-- TextService:GetTextBoundsAsync: GetTextBoundsParams' defaults, its
-- bounds against a TextLabel's TextBounds for the same text, Width 0 and
-- a narrow Width, rich text, and what bad arguments raise
-- (docs/behavior/gui/text.md#measuring-text).
local TextService = game:GetService("TextService")

local defaults = Instance.new("GetTextBoundsParams")
log("defaults", defaults.Name, defaults.Text == "", defaults.Size, defaults.Width, defaults.RichText)
log("default font", defaults.Font.Family, defaults.Font.Weight, defaults.Font.Style)

local function bounds(props)
	local p = Instance.new("GetTextBoundsParams")
	p.Font = Font.fromEnum(Enum.Font.SourceSans)
	p.Size = 24
	for k, value in props do
		p[k] = value
	end
	local ok, b = pcall(TextService.GetTextBoundsAsync, TextService, p)
	p:Destroy()
	return ok, b
end

local hello = "Hello, world!"
log("hello", bounds({ Text = hello }))
log("width 0 two lines", bounds({ Text = "A\nB" }))
log("width 60", bounds({ Text = hello, Width = 60 }))
log("width 1", bounds({ Text = "Hi", Width = 1 }))
log("rich", bounds({ Text = "<b>Hello</b>, world!", RichText = true }))
log("rich off", bounds({ Text = "<b>Hello</b>, world!" }))
log("rich size", bounds({ Text = 'A<font size="40">B</font>', RichText = true }))
log("empty", bounds({ Text = "" }))
log("default size", bounds({ Text = hello, Size = defaults.Size }))

log("missing", pcall(function() return TextService:GetTextBoundsAsync() end))
log("nil", pcall(function() return TextService:GetTextBoundsAsync(nil) end))
local folder = Instance.new("Folder")
log("folder", pcall(function() return TextService:GetTextBoundsAsync(folder) end))
folder:Destroy()
log("string", pcall(function() return TextService:GetTextBoundsAsync("x") end))
defaults:Destroy()
