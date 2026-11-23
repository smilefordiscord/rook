-- Selecting a TextBox's text with the mouse: a drag across an unfocused
-- box (it focuses, the selection running from the press to the release),
-- a drag in the focused box from right to left, one that leaves the box
-- to the right, and a plain click dropping the selection. Each release
-- logs CursorPosition, SelectionStart and the text between them. T at
-- (100, 100) is 400 x 50, its 20-pixel text from the left.
--! client input 115,125 down 260,125 up 380,125 down 160,125 up 200,125 down 600,125 up 300,125 down up 600,400 down up
local UserInputService = game:GetService("UserInputService")
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.ResetOnSpawn = false
screen.DisplayOrder = 1000000

local t = Instance.new("TextBox")
t.Name = "T"
t.Position = UDim2.fromOffset(100, 100)
t.Size = UDim2.fromOffset(400, 50)
t.Text = "alpha beta gamma delta"
t.TextSize = 20
t.Font = Enum.Font.Arial
t.TextXAlignment = Enum.TextXAlignment.Left
t.ClearTextOnFocus = false
t.Parent = screen

local function selection()
	local c, s = t.CursorPosition, t.SelectionStart
	local picked = ""
	if c ~= -1 and s ~= -1 then
		picked = string.sub(t.Text, math.min(c, s), math.max(c, s) - 1)
	end
	return c, s, string.format("%q", picked)
end
local releases = 0
local connections = {}
table.insert(
	connections,
	UserInputService.InputEnded:Connect(function(input)
		if input.UserInputType == Enum.UserInputType.MouseButton1 then
			releases += 1
			log("release", releases, "focused", t:IsFocused(), selection())
		end
	end)
)
screen.Parent = player:WaitForChild("PlayerGui")

local start = os.clock()
while releases < 5 and os.clock() - start < 20 do
	task.wait()
end
task.wait(0.1)
log("end", t.Text)
for _, c in connections do
	c:Disconnect()
end
screen:Destroy()
