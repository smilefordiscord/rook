-- How wide one line is in fonts whose files leave a line gap (hhea
-- lineGap) and one that doesn't, at TextSize 100: a face's scale, to the
-- nearest 10 pixels (finer, each glyph's rounding shows; the gap is 2.5%).
--! client
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.Parent = player:WaitForChild("PlayerGui")

local TEXT = "Featured Hamburgefonstiv"
local fonts = {
	-- lineGap 28 of 1000 (Ubuntu-Regular.ttf)
	"Ubuntu",
	-- 67 of 2048 (Arimo, Arial's stand-in)
	"Arimo",
	-- none
	"SourceSans",
}
for _, name in fonts do
	local label = Instance.new("TextLabel")
	label.Font = Enum.Font[name]
	label.TextSize = 100
	label.Text = TEXT
	label.Size = UDim2.fromOffset(4000, 200)
	label.Parent = screen
	task.wait()
	log(name, "width", math.round(label.TextBounds.X / 10) * 10, "height", label.TextBounds.Y)
	label:Destroy()
end

screen:Destroy()
