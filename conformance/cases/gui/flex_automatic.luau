-- A flexed list item that grows by AutomaticSize across the list: its
-- content is measured at the length flex gave it, and the line, the list's
-- AbsoluteContentSize and a parent that grows too follow (a column of
-- wrapped text beside an image, filling the rest of a row). The item holds
-- a square sized from its length (SizeConstraint), so it's only as thick
-- as it is long once flexed.
--! client
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.Parent = player:WaitForChild("PlayerGui")

-- Rounded, and + 0 turns -0 (float noise) into 0.
local function v(p)
	return string.format("%g,%g", math.round(p.X * 100) / 100 + 0, math.round(p.Y * 100) / 100 + 0)
end

local function case(tag, horizontal, rowAuto, itemAuto, mode, align)
	local row = Instance.new("Frame")
	row.Size = if horizontal then UDim2.fromOffset(300, 20) else UDim2.fromOffset(20, 300)
	row.AutomaticSize = rowAuto
	local list = Instance.new("UIListLayout")
	list.FillDirection = if horizontal then Enum.FillDirection.Horizontal else Enum.FillDirection.Vertical
	list.SortOrder = Enum.SortOrder.LayoutOrder
	list.VerticalAlignment = align
	list.HorizontalAlignment = Enum.HorizontalAlignment.Center
	list.Parent = row
	local fixed = Instance.new("Frame")
	fixed.LayoutOrder = 1
	fixed.Size = if horizontal then UDim2.fromOffset(100, 50) else UDim2.fromOffset(50, 100)
	fixed.Parent = row
	local item = Instance.new("Frame")
	item.LayoutOrder = 2
	item.Size = UDim2.fromOffset(0, 0)
	item.AutomaticSize = itemAuto
	local flex = Instance.new("UIFlexItem")
	flex.FlexMode = mode
	flex.Parent = item
	Instance.new("UIListLayout").Parent = item
	local square = Instance.new("Frame")
	square.Size = UDim2.fromScale(1, 1)
	square.SizeConstraint = if horizontal then Enum.SizeConstraint.RelativeXX else Enum.SizeConstraint.RelativeYY
	square.Parent = item
	item.Parent = row
	row.Parent = screen
	task.wait()
	task.wait()
	local at = row.AbsolutePosition
	log(
		tag,
		"row",
		v(row.AbsoluteSize),
		"fixed",
		v(fixed.AbsolutePosition - at) .. "/" .. v(fixed.AbsoluteSize),
		"item",
		v(item.AbsolutePosition - at) .. "/" .. v(item.AbsoluteSize),
		"square",
		v(square.AbsoluteSize),
		"content",
		v(list.AbsoluteContentSize)
	)
	row:Destroy()
end

local A = Enum.AutomaticSize
local M = Enum.UIFlexMode
local V = Enum.VerticalAlignment
case("fill, grows Y", true, A.None, A.Y, M.Fill, V.Top)
case("fill, grows Y, row grows Y, centered", true, A.Y, A.Y, M.Fill, V.Center)
case("grow, grows Y, row grows Y, centered", true, A.Y, A.Y, M.Grow, V.Center)
case("fill, grows XY, row grows Y", true, A.Y, A.XY, M.Fill, V.Top)
case("fill, no AutomaticSize, row grows Y", true, A.Y, A.None, M.Fill, V.Top)
case("vertical, fill, grows X, row grows X", false, A.X, A.X, M.Fill, V.Top)

screen:Destroy()
