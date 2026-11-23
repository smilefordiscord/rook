-- Where a click puts a TextBox's caret within a character: clicks
-- across the first two "m"s of "mmmm" in 60-pixel Arial, each 44 pixels
-- wide (edges at 0, 44 and 88 pixels into the text). T at (100, 100) is
-- 400 x 80, its text from the left. Each click logs where it was in the
-- text and the CursorPosition it gave.
--! client input 118,140 down up 122,140 down up 124,140 down up 126,140 down up 128,140 down up 129,140 down up 130,140 down up 131,140 down up 132,140 down up 134,140 down up 136,140 down up 166,140 down up 170,140 down up 172,140 down up 174,140 down up 176,140 down up
local UserInputService = game:GetService("UserInputService")
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.ResetOnSpawn = false
screen.DisplayOrder = 1000000

local t = Instance.new("TextBox")
t.Name = "T"
t.Position = UDim2.fromOffset(100, 100)
t.Size = UDim2.fromOffset(400, 80)
t.Text = "mmmm"
t.TextSize = 60
t.Font = Enum.Font.Arial
t.TextXAlignment = Enum.TextXAlignment.Left
t.ClearTextOnFocus = false
t.Parent = screen

local clicks = 0
local connections = {}
table.insert(
	connections,
	UserInputService.InputEnded:Connect(function(input)
		if input.UserInputType == Enum.UserInputType.MouseButton1 then
			clicks += 1
			log("click", clicks, "at", input.Position.X - 100, "caret", t.CursorPosition)
		end
	end)
)
screen.Parent = player:WaitForChild("PlayerGui")
local start = os.clock()
while clicks < 16 and os.clock() - start < 30 do
	task.wait()
end
for _, c in connections do
	c:Disconnect()
end
screen:Destroy()
