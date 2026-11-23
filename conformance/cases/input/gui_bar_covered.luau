-- A ScrollingFrame's bar under other GUI, and one with no bar: dragging
-- the thumb of A under a full-screen Frame (not Active) from a ScreenGui
-- above, of B under an Active Frame, of C under a TextButton; then the
-- wheel over D, whose bars are 0 thick. A, B and C at (100, 100),
-- (350, 100) and (600, 100) are 200 x 200 with 1000-high canvases and
-- 12-pixel bars (each thumb at the top of its bar, 288 to 300 across);
-- D at (100, 350) is 200 x 100 with a 400-high canvas.
--! client input 294,110 down 294,160 up 544,110 down 544,160 up 794,110 down 794,160 up 200,400 wheeldown wait wait wait
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.ResetOnSpawn = false
screen.DisplayOrder = 1000000
local over = Instance.new("ScreenGui")
over.ResetOnSpawn = false
over.DisplayOrder = 1000001

local function scroller(name, x, y, h, canvas, thickness)
	local f = Instance.new("ScrollingFrame")
	f.Name = name
	f.Position = UDim2.fromOffset(x, y)
	f.Size = UDim2.fromOffset(200, h)
	f.CanvasSize = UDim2.fromOffset(0, canvas)
	f.ScrollBarThickness = thickness
	f.Parent = screen
	return f
end
local a = scroller("A", 100, 100, 200, 1000, 12)
local b = scroller("B", 350, 100, 200, 1000, 12)
local c = scroller("C", 600, 100, 200, 1000, 12)
local d = scroller("D", 100, 350, 100, 400, 0)

local veil = Instance.new("Frame")
veil.Name = "Veil"
veil.Size = UDim2.fromScale(1, 1)
veil.BackgroundTransparency = 1
veil.Active = false
veil.Parent = over
local block = Instance.new("Frame")
block.Name = "Block"
block.Position = UDim2.fromOffset(530, 90)
block.Size = UDim2.fromOffset(40, 100)
block.BackgroundTransparency = 0.5
block.Active = true
block.Parent = over
local button = Instance.new("TextButton")
button.Name = "Button"
button.Position = UDim2.fromOffset(780, 90)
button.Size = UDim2.fromOffset(40, 100)
button.Text = ""
button.Parent = over

local function y(f)
	return math.round(f.CanvasPosition.Y)
end
local released = 0
local connections = {}
table.insert(connections, game:GetService("UserInputService").InputEnded:Connect(function(input)
	if input.UserInputType == Enum.UserInputType.MouseButton1 then
		released += 1
		log("released", released, "A", y(a), "B", y(b), "C", y(c))
	end
end))
table.insert(connections, game:GetService("UserInputService").InputChanged:Connect(function(input)
	if input.UserInputType == Enum.UserInputType.MouseWheel then
		task.delay(0.4, function()
			log("wheel over D", y(d), "window", d.AbsoluteWindowSize, "canvas", d.AbsoluteCanvasSize)
			released = 99
		end)
	end
end))
screen.Parent = player:WaitForChild("PlayerGui")
over.Parent = player.PlayerGui
local start = os.clock()
while released < 99 and os.clock() - start < 20 do
	task.wait()
end
task.wait(0.1)
for _, conn in connections do
	conn:Disconnect()
end
screen:Destroy()
over:Destroy()
