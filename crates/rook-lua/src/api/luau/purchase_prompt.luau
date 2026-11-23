-- Rook's own purchase prompt, in the local player's CoreGui: a panel
-- over the dimmed game with the item, its price, a note that the purchase
-- is a test, and a row of buttons. Its look is Rook's (Approximation: not
-- laid out from Roblox's prompt; docs/behavior/scripting/marketplace.md).
-- `ask` shows it and waits for a button; `close` hides it.
local std = ...
local game, Enum, UDim, UDim2, Color3, Vector2 =
	std.game, std.Enum, std.UDim, std.UDim2, std.Color3, std.Vector2
local new, ipairs = std.new_instance, std.ipairs

local WHITE = Color3.new(1, 1, 1)
local BUY = Color3.fromRGB(51, 95, 255)
local OTHER = Color3.fromRGB(70, 73, 77)

local api = {}
local screen, title, body, row

local function text(parent, name, position, size, textSize, color)
	local l = new("TextLabel")
	l.Name = name
	l.BackgroundTransparency = 1
	l.Position = position
	l.Size = size
	l.Font = Enum.Font.BuilderSans
	l.TextSize = textSize
	l.TextColor3 = color
	l.TextWrapped = true
	l.TextXAlignment = Enum.TextXAlignment.Left
	l.TextYAlignment = Enum.TextYAlignment.Top
	l.Parent = parent
	return l
end

local function corner(parent, radius)
	local c = new("UICorner")
	c.CornerRadius = UDim.new(0, radius)
	c.Parent = parent
end

local function build()
	screen = new("ScreenGui")
	screen.Name = "PurchasePrompt"
	screen.IgnoreGuiInset = true
	screen.Enabled = false
	-- Dims the game and takes the clicks meant for it.
	local shade = new("Frame")
	shade.Name = "Shade"
	shade.Size = UDim2.fromScale(1, 1)
	shade.BackgroundColor3 = Color3.new(0, 0, 0)
	shade.BackgroundTransparency = 0.5
	shade.BorderSizePixel = 0
	shade.Active = true
	shade.Parent = screen
	local panel = new("Frame")
	panel.Name = "Panel"
	panel.AnchorPoint = Vector2.new(0.5, 0.5)
	panel.Position = UDim2.fromScale(0.5, 0.5)
	panel.Size = UDim2.fromOffset(400, 220)
	panel.BackgroundColor3 = Color3.fromRGB(35, 37, 39)
	panel.BorderSizePixel = 0
	panel.Active = true
	panel.Parent = screen
	corner(panel, 8)
	title = text(panel, "Title", UDim2.fromOffset(20, 16), UDim2.new(1, -40, 0, 28), 22, WHITE)
	title.Font = Enum.Font.BuilderSansBold
	title.TextTruncate = Enum.TextTruncate.AtEnd
	body = text(panel, "Body", UDim2.fromOffset(20, 52), UDim2.new(1, -40, 0, 72), 18, Color3.fromRGB(220, 220, 220))
	local note = text(panel, "Note", UDim2.fromOffset(20, 128), UDim2.new(1, -40, 0, 20), 14, Color3.fromRGB(160, 160, 160))
	note.Text = "Test purchase: your account will not be charged."
	row = new("Frame")
	row.Name = "Buttons"
	row.BackgroundTransparency = 1
	row.Position = UDim2.new(0, 20, 1, -56)
	row.Size = UDim2.new(1, -40, 0, 40)
	row.Parent = panel
	local list = new("UIListLayout")
	list.FillDirection = Enum.FillDirection.Horizontal
	list.HorizontalAlignment = Enum.HorizontalAlignment.Right
	list.SortOrder = Enum.SortOrder.LayoutOrder
	list.Padding = UDim.new(0, 12)
	list.Parent = row
	screen.Parent = game:GetService("CoreGui")
end

-- Show `heading` and `message` with a button for each of `choices`, the
-- first one highlighted; returns the choice clicked.
function api.ask(heading, message, choices)
	-- Game scripts can reach CoreGui: a prompt whose buttons were taken
	-- out is built again, or the player could never answer it.
	if not screen or not screen:IsDescendantOf(game) or not row:IsDescendantOf(screen) then
		if screen then
			screen:Destroy()
		end
		build()
	end
	title.Text = heading
	body.Text = message
	for _, child in ipairs(row:GetChildren()) do
		if child:IsA("GuiButton") then
			child:Destroy()
		end
	end
	local chosen = new("BindableEvent")
	for i, choice in ipairs(choices) do
		local b = new("TextButton")
		b.Name = choice
		b.Text = choice
		b.LayoutOrder = i
		b.Size = UDim2.fromOffset(120, 40)
		b.BackgroundColor3 = if i == 1 then BUY else OTHER
		b.BorderSizePixel = 0
		b.TextColor3 = WHITE
		b.Font = Enum.Font.BuilderSansBold
		b.TextSize = 18
		-- Frees a mouse the game holds, as any shown Modal button does.
		b.Modal = true
		corner(b, 8)
		b.Activated:Connect(function()
			chosen:Fire(choice)
		end)
		b.Parent = row
	end
	screen.Enabled = true
	local choice = chosen.Event:Wait()
	chosen:Destroy()
	return choice
end

function api.close()
	if screen then
		screen.Enabled = false
	end
end

return api
