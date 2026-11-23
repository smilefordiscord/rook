-- TextScaled text in an object that grows by AutomaticSize: how far it
-- grows and what TextBounds reads (labels 0 wide by `Size`, growing on X
-- to hold the text scaled to their height; DashBlox 2's shop names them so).
--! client
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.Parent = player:WaitForChild("PlayerGui")

-- To the nearest 10 pixels: the rule, not each glyph's rounding (which
-- docs/behavior/gui/text.md#widths covers).
local function v(p)
	return string.format("%g,%g", math.round(p.X / 10) * 10, math.round(p.Y / 10) * 10)
end

local function case(tag, props, extra)
	local label = Instance.new("TextLabel")
	label.TextScaled = true
	label.Font = Enum.Font.SourceSans
	label.TextSize = 14
	for k, value in props do
		(label :: any)[k] = value
	end
	if extra then
		extra(label)
	end
	label.Parent = screen
	task.wait()
	task.wait()
	log(tag, "size", v(label.AbsoluteSize), "bounds", v(label.TextBounds), "fits", label.TextFits)
	label:Destroy()
end

local X, Y, XY = Enum.AutomaticSize.X, Enum.AutomaticSize.Y, Enum.AutomaticSize.XY
for _, h in { 17, 22, 26, 60, 140 } do
	case(`X, {h} tall`, { Text = "Featured", AutomaticSize = X, Size = UDim2.fromOffset(0, h) })
end
case("X, 22 tall, wrapped", { Text = "Featured", AutomaticSize = X, Size = UDim2.fromOffset(0, 22), TextWrapped = true })
case("X, 22 tall, two words", { Text = "New icons in 22h 56m", AutomaticSize = X, Size = UDim2.fromOffset(0, 22) })
case("X, 22 tall, from 50 wide", { Text = "Featured", AutomaticSize = X, Size = UDim2.fromOffset(50, 22) })
case("X, 22 tall, from 300 wide", { Text = "Featured", AutomaticSize = X, Size = UDim2.fromOffset(300, 22) })
case("X, 22 tall, padded 5", { Text = "Featured", AutomaticSize = X, Size = UDim2.fromOffset(0, 22) }, function(l)
	local pad = Instance.new("UIPadding")
	pad.PaddingLeft = UDim.new(0, 5)
	pad.PaddingRight = UDim.new(0, 5)
	pad.PaddingTop = UDim.new(0, 5)
	pad.PaddingBottom = UDim.new(0, 5)
	pad.Parent = l
end)
case("X, 22 tall, max 10", { Text = "Featured", AutomaticSize = X, Size = UDim2.fromOffset(0, 22) }, function(l)
	local c = Instance.new("UITextSizeConstraint")
	c.MaxTextSize = 10
	c.Parent = l
end)
case("Y, 100 wide", { Text = "Featured", AutomaticSize = Y, Size = UDim2.fromOffset(100, 0) })
case("Y, 100 wide, 30 tall", { Text = "Featured", AutomaticSize = Y, Size = UDim2.fromOffset(100, 30) })
case("XY, nothing", { Text = "Featured", AutomaticSize = XY, Size = UDim2.fromOffset(0, 0) })
case("XY, 22 tall", { Text = "Featured", AutomaticSize = XY, Size = UDim2.fromOffset(0, 22) })
case("X, 22 tall, scaled off", { Text = "Featured", AutomaticSize = X, Size = UDim2.fromOffset(0, 22), TextScaled = false })

screen:Destroy()
