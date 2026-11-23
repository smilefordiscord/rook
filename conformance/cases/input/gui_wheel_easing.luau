-- How the wheel eases a ScrollingFrame, frame by frame: one notch down, a
-- flick of three at once, one notch back up, and a flick down that runs
-- into the canvas's end. Each turn logs where it started and landed, the
-- frames it moved on and the spring its frames fit (timed from its first
-- frame, so it holds however late the MCP's steps come). S at (100, 100) is 200 x 200
-- with a 1000-high canvas (800 to scroll); it starts 300 down, so the
-- last flick stops at the end, 100 short of where it was headed.
-- `print` shows each frame's position, for comparing the curves by eye.
--! client input 200,200 wheeldown wait wait wait wheeldown*3 wait wait wait wheelup wait wait wait wheeldown*3 wait wait wait
local UserInputService = game:GetService("UserInputService")
local RunService = game:GetService("RunService")
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.ResetOnSpawn = false
-- Over whatever the place shows, so the wheel reaches S.
screen.DisplayOrder = 1000000
local s = Instance.new("ScrollingFrame")
s.Name = "S"
s.Position = UDim2.fromOffset(100, 100)
s.Size = UDim2.fromOffset(200, 200)
s.CanvasSize = UDim2.fromOffset(0, 1000)
s.Parent = screen

local function y()
	return s.CanvasPosition.Y
end
local function n(v)
	return math.round(v * 10) / 10 + 0
end

-- The turn under way: the notches in it, where it started and each
-- frame's (time since it started, position) once its last notch came.
local turn = nil
local turns = 0
local connections = {}
local function finish(t)
	local samples = t.samples
	local landed = samples[#samples][2]
	-- The frames it moved on. The canvas moves once a drawn frame, and
	-- Heartbeat can come more often (Studio's at 240 a second while its
	-- window draws 60), so the spring is timed from the first move less a
	-- drawn frame, the usual time between moves.
	local moves = {}
	for i = 2, #samples do
		if samples[i][2] ~= samples[i - 1][2] then
			table.insert(moves, samples[i])
		end
	end
	local gaps = {}
	for i = 2, #moves do
		table.insert(gaps, moves[i][1] - moves[i - 1][1])
	end
	table.sort(gaps)
	local frame = gaps[(#gaps + 1) // 2] or 1 / 60
	local first = (moves[1] and moves[1][1] or 0) - frame
	-- The frequency of the critically damped spring, leaving
	-- (1 + wt) e^(-wt) of the way after t seconds, that fits the frames
	-- before it landed best.
	local total = landed - t.from
	local best, err = nil, math.huge
	for hz = 1, 20, 0.01 do
		local w = hz * 2 * math.pi
		local sum = 0
		for i = 1, #moves - 1 do
			local time = moves[i][1] - first
			local left = (landed - moves[i][2]) / total
			local model = (1 + w * time) * math.exp(-w * time)
			sum += (left - model) ^ 2
		end
		if sum < err then
			best, err = hz, sum
		end
	end
	log(
		"turn",
		t.number,
		"notches",
		t.notches,
		"from",
		n(t.from),
		"to",
		n(landed),
		-- To the nearest 20th of a second, whatever the frame rate.
		"lands after",
		moves[1] and math.round((moves[#moves][1] - first) * 20) / 20 or "none",
		-- To the nearest half hertz: frame times in a playtest wander.
		"spring hz",
		best and math.round(best * 2) / 2 or "none",
		-- How far the frames stray from that spring, in hundredths of
		-- the way: 1 or less is the spring.
		"off",
		math.ceil(math.sqrt(err / math.max(#moves - 1, 1)) * 100)
	)
	local trace = {}
	for _, sample in moves do
		table.insert(trace, string.format("%.3f:%g", sample[1], n(sample[2])))
	end
	print("wheel easing turn", t.number, table.concat(trace, " "))
end

table.insert(
	connections,
	UserInputService.InputChanged:Connect(function(input, processed)
		if input.UserInputType ~= Enum.UserInputType.MouseWheel then
			return
		end
		if not turn then
			turns += 1
			turn = { number = turns, notches = 0, from = y(), samples = {}, since = 0, quiet = 0 }
			log("turn", turns, "begins", "processed", processed)
		end
		turn.notches += input.Position.Z
		-- The curve that counts starts with the turn's last notch.
		turn.samples = { { 0, y() } }
		turn.since = 0
		turn.quiet = 0
	end)
)
table.insert(
	connections,
	RunService.Heartbeat:Connect(function(dt)
		local t = turn
		if not t then
			return
		end
		t.since += dt
		local last = t.samples[#t.samples][2]
		table.insert(t.samples, { t.since, y() })
		-- Landed: 0.3 s without moving (the next step comes 0.2 s after
		-- it lands at the soonest).
		t.quiet = if y() == last then t.quiet + dt else 0
		if t.quiet >= 0.3 then
			turn = nil
			-- The still frames after it landed don't count.
			local kept = {}
			for _, sample in t.samples do
				table.insert(kept, sample)
				if sample[2] == y() and #kept > 1 then
					break
				end
			end
			t.samples = kept
			finish(t)
		end
	end)
)
screen.Parent = player:WaitForChild("PlayerGui")
s.CanvasPosition = Vector2.new(0, 300)
log("window", s.AbsoluteWindowSize, "canvas", s.AbsoluteCanvasSize, "at", y())

local start = os.clock()
while turns < 4 or turn do
	if os.clock() - start > 30 then
		log("timed out after", turns, "turns")
		break
	end
	task.wait()
end
log("end", n(y()))
for _, c in connections do
	c:Disconnect()
end
screen:Destroy()
