-- HapticService on a playtest server, where no gamepad is connected:
-- what IsVibrationSupported, IsMotorSupported, SetMotor and GetMotor
-- return and how they take their arguments, and Enum.VibrationMotor.
-- docs/behavior/input/gamepads.md#motors.
local function caught(f, ...)
	local r = table.pack(pcall(f, ...))
	if r[1] then
		local s = {}
		for i = 2, r.n do
			local v = r[i]
			-- NaN's sign isn't part of what's measured.
			s[#s + 1] = (type(v) == "number" and v ~= v) and "nan" or type(v) == "table" and "<table>" or tostring(v)
		end
		return "ok", r.n - 1, table.concat(s, " ")
	end
	local e = r[2]
	return "error", type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end

local HS = game:GetService("HapticService")
log("service", HS.ClassName, HS.Name)

local items = {}
for _, item in Enum.VibrationMotor:GetEnumItems() do
	items[#items + 1] = item.Name .. "=" .. item.Value
end
log("VibrationMotor", table.concat(items, " "))

local G1, G2 = Enum.UserInputType.Gamepad1, Enum.UserInputType.Gamepad2
local Large, Small = Enum.VibrationMotor.Large, Enum.VibrationMotor.Small

local function call(label, name, ...)
	local args = table.pack(...)
	log(label, caught(function()
		return HS[name](HS, table.unpack(args, 1, args.n))
	end))
end

-- Supported?
for _, t in { G1, G2, Enum.UserInputType.Gamepad8, Enum.UserInputType.Keyboard, Enum.UserInputType.Touch } do
	call("IsVibrationSupported " .. t.Name, "IsVibrationSupported", t)
end
call("IsVibrationSupported string", "IsVibrationSupported", "Gamepad1")
call("IsVibrationSupported number", "IsVibrationSupported", 3)
call("IsVibrationSupported bad string", "IsVibrationSupported", "Pad")
call("IsVibrationSupported other enum", "IsVibrationSupported", Enum.KeyCode.A)
call("IsVibrationSupported nil", "IsVibrationSupported", nil)
call("IsVibrationSupported none", "IsVibrationSupported")
for _, m in Enum.VibrationMotor:GetEnumItems() do
	call("IsMotorSupported Gamepad1 " .. m.Name, "IsMotorSupported", G1, m)
end
call("IsMotorSupported Keyboard Large", "IsMotorSupported", Enum.UserInputType.Keyboard, Large)
call("IsMotorSupported string motor", "IsMotorSupported", G1, "Small")
call("IsMotorSupported number motor", "IsMotorSupported", G1, 1)
call("IsMotorSupported bad motor", "IsMotorSupported", G1, "Huge")
call("IsMotorSupported nil motor", "IsMotorSupported", G1, nil)
call("IsMotorSupported no motor", "IsMotorSupported", G1)

-- GetMotor before anything is set.
call("GetMotor unset", "GetMotor", G2, Small)
call("GetMotor unset keyboard", "GetMotor", Enum.UserInputType.Keyboard, Large)

-- SetMotor, then GetMotor.
local function set_get(label, t, m, ...)
	local args = table.pack(...)
	log("SetMotor " .. label, caught(function()
		return HS:SetMotor(t, m, table.unpack(args, 1, args.n))
	end))
	log("GetMotor " .. label, caught(function()
		return HS:GetMotor(t, m)
	end))
end
set_get("0.5", G1, Large, 0.5)
set_get("1", G1, Small, 1)
set_get("2.5", G1, Large, 2.5)
set_get("-1", G1, Large, -1)
set_get("nan", G1, Large, 0 / 0)
set_get("huge", G1, Large, math.huge)
set_get("tuple", G1, Large, 0.25, 0.75, "x")
set_get("string number", G1, Large, "0.3")
set_get("bad string", G1, Large, "x")
set_get("boolean", G1, Large, true)
set_get("table", G1, Large, { 0.4 })
set_get("nil", G1, Large, nil)
set_get("none", G1, Large)
set_get("LeftTrigger", G1, Enum.VibrationMotor.LeftTrigger, 0.6)
set_get("RightHand", G1, Enum.VibrationMotor.RightHand, 0.7)
set_get("Gamepad8", Enum.UserInputType.Gamepad8, Small, 0.8)
set_get("Keyboard", Enum.UserInputType.Keyboard, Large, 0.9)
set_get("Touch", Enum.UserInputType.Touch, Small, 0.1)
set_get("string type", "Gamepad3", "Large", 0.2)
log("GetMotor other slot", caught(function()
	return HS:GetMotor(G2, Large)
end))
log("SetMotor nil type", caught(function()
	HS:SetMotor(nil, Large, 1)
end))
log("SetMotor nil motor", caught(function()
	HS:SetMotor(G1, nil, 1)
end))
log("SetMotor wrong enum", caught(function()
	HS:SetMotor(G1, Enum.KeyCode.A, 1)
end))
log("GetMotor no motor", caught(function()
	return HS:GetMotor(G1)
end))
log("Supported after set", caught(function()
	return HS:IsVibrationSupported(G1), HS:IsMotorSupported(G1, Large)
end))

-- Leave every motor at rest.
for _, t in { G1, Enum.UserInputType.Gamepad3, Enum.UserInputType.Gamepad8 } do
	for _, m in Enum.VibrationMotor:GetEnumItems() do
		pcall(function()
			HS:SetMotor(t, m, 0)
		end)
	end
end
log("reset", caught(function()
	return HS:GetMotor(G1, Large)
end))
