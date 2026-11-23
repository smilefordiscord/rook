use crate::tests::*;

#[test]
fn sorted_map_sets_gets_and_ranges_in_order() {
    expect(
        r##"
        local mss = game:GetService("MemoryStoreService")
        local map = mss:GetSortedMap("Board")
        print(map == mss:GetSortedMap("Board"), map.ClassName, map.Name)
        print(map:SetAsync("player7", 7, 60), map:SetAsync("player7", { n = 7 }, 60))
        map:SetAsync("player3", 3, 60, 3.14)
        map:SetAsync("player6", 6, 60, "someString")
        map:SetAsync("player1", 1, 60, -1)
        map:SetAsync("player0", 0, 60)
        print(map:GetAsync("player3"))
        print(map:GetAsync("player7").n, select("#", map:GetAsync("missing")))
        local keys = {}
        for _, item in map:GetRangeAsync(Enum.SortDirection.Ascending, 10) do
            table.insert(keys, item.key .. "=" .. tostring(item.sortKey))
        end
        print(table.concat(keys, " "))
        local down = map:GetRangeAsync(Enum.SortDirection.Descending, 2, { key = "player1", sortKey = -1 })
        print(#down, down[1].key, down[2].key)
        local up = map:GetRangeAsync(Enum.SortDirection.Ascending, 10, nil, { sortKey = "someString" })
        print(#up, up[1].key, up[2].key)
        print(map:GetSizeAsync())
        map:RemoveAsync("player0")
        print(map:GetSizeAsync(), map:GetAsync("player0"))
        "##,
        0,
        &[
            "true MemoryStoreSortedMap Board",
            "false true",
            "3 3.14",
            "7 2",
            "player1=-1 player3=3.14 player6=someString player0=nil player7=nil",
            "2 player7 player0",
            "2 player1 player3",
            "5",
            "4 nil nil",
        ],
    );
}

/// The transform gets the old value and sort key; nil cancels.
#[test]
fn update_async_transforms_or_cancels() {
    expect(
        r#"
        local mss = game:GetService("MemoryStoreService")
        local map = mss:GetSortedMap("Bids")
        print(map:UpdateAsync("item", function(old, sort)
            print("old", old, sort)
            return { bid = 5 }, 5
        end, 30).bid)
        print(map:UpdateAsync("item", function(old) return nil end, 30))
        print(map:GetAsync("item").bid, select(2, map:GetAsync("item")))
        print(pcall(function()
            map:UpdateAsync("item", function() error("no") end, 30)
        end))
        local hash = mss:GetHashMap("Inventory")
        print(hash:UpdateAsync("wood", function(old) return (old or 0) + 3 end, 30))
        print(hash:UpdateAsync("wood", function(old) return old + 3 end, 30))
        "#,
        0,
        &[
            "old nil nil",
            "5",
            "nil",
            "5 5",
            "false TransformCallbackFailed: Failed to invoke transformation callback function.",
            "3",
            "6",
        ],
    );
}

#[test]
fn items_expire_on_the_session_clock() {
    expect(
        r#"
        local mss = game:GetService("MemoryStoreService")
        local map = mss:GetSortedMap("Short")
        local hash = mss:GetHashMap("Short")
        map:SetAsync("a", 1, 1)
        hash:SetAsync("a", 1, 2)
        print(map:GetAsync("a"), hash:GetAsync("a"))
        task.wait(1.5)
        print(map:GetAsync("a"), hash:GetAsync("a"), map:GetSizeAsync())
        task.wait(1)
        print(hash:GetAsync("a"))
        "#,
        200,
        &["1 1", "nil 1 0", "nil"],
    );
}

#[test]
fn queue_reads_by_priority_and_hides_what_it_reads() {
    expect(
        r#"
        local mss = game:GetService("MemoryStoreService")
        local queue = mss:GetQueue("Match", 1)
        queue:AddAsync("a", 60)
        queue:AddAsync("b", 60)
        queue:AddAsync("vip", 60, 10)
        local items, id = queue:ReadAsync(2)
        print(items[1], items[2], typeof(id))
        print(queue:GetSizeAsync(), queue:GetSizeAsync(true))
        local rest, id2 = queue:ReadAsync(5, true, 0)
        print(#rest, id2)
        queue:RemoveAsync(id)
        print(queue:GetSizeAsync())
        -- Unremoved reads show again once the queue's invisibility runs out.
        queue:ReadAsync(1)
        print(queue:GetSizeAsync(true))
        task.wait(1.5)
        print(queue:GetSizeAsync(true), queue:ReadAsync(1)[1])
        "#,
        120,
        &["vip a string", "3 1", "0 nil", "1", "0", "1 b"],
    );
}

/// A read's id removes only from the queue that read: each queue's first
/// read once had the same id, so one queue's removed another's items.
#[test]
fn a_read_id_removes_only_from_its_queue() {
    expect(
        r#"
        local mss = game:GetService("MemoryStoreService")
        local a, b = mss:GetQueue("A"), mss:GetQueue("B")
        a:AddAsync("a", 60)
        b:AddAsync("b", 60)
        local _, idA = a:ReadAsync(1)
        local _, idB = b:ReadAsync(1)
        print(idA ~= idB)
        b:RemoveAsync(idA)
        print(a:GetSizeAsync(), b:GetSizeAsync())
        "#,
        0,
        &["true", "1 1"],
    );
}

/// Reads are tried again every two seconds until `waitTimeout`.
#[test]
fn read_async_waits_for_items() {
    expect(
        r#"
        local queue = game:GetService("MemoryStoreService"):GetQueue("Wait")
        task.delay(3, function() queue:AddAsync("late", 60) end)
        local start = time()
        local items = queue:ReadAsync(1, false, 10)
        print(items[1], math.floor(time() - start + 0.5))
        start = time()
        local none = queue:ReadAsync(1, false, 3)
        print(#none, math.floor(time() - start + 0.5))
        "#,
        600,
        &["late 4", "0 3"],
    );
}

#[test]
fn hash_map_lists_items_in_pages() {
    expect(
        r#"
        local hash = game:GetService("MemoryStoreService"):GetHashMap("Shared")
        for i = 1, 5 do
            print(hash:SetAsync("k" .. i, i * 10, 60))
        end
        print(hash:SetAsync("k1", 11, 60))
        local pages = hash:ListItemsAsync(2)
        print(pages.ClassName, pages.IsFinished)
        local seen = {}
        while true do
            for _, entry in pages:GetCurrentPage() do
                table.insert(seen, entry.key .. "=" .. entry.value)
            end
            if pages.IsFinished then break end
            pages:AdvanceToNextPageAsync()
        end
        print(table.concat(seen, " "))
        print(pcall(function() pages:AdvanceToNextPageAsync() end))
        hash:RemoveAsync("k1")
        print(hash:GetAsync("k1"))
        "#,
        0,
        &[
            "false",
            "false",
            "false",
            "false",
            "false",
            "true",
            "MemoryStoreHashMapPages false",
            "k1=11 k2=20 k3=30 k4=40 k5=50",
            "false No pages to advance to",
            "nil",
        ],
    );
}

/// NaN and infinite durations raise too: a NaN invisibility timeout once
/// hid nothing, and a NaN or infinite `waitTimeout` waited forever.
#[test]
fn arguments_are_checked_against_roblox_limits() {
    expect(
        r#"
        local mss = game:GetService("MemoryStoreService")
        local map = mss:GetSortedMap("Limits")
        local function try(f) print((select(2, pcall(f)))) end
        try(function() map:SetAsync("k", 1, 3888001) end)
        try(function() map:SetAsync("k", 1, -1) end)
        try(function() map:SetAsync(string.rep("k", 129), 1, 10) end)
        try(function() map:SetAsync("k", workspace, 10) end)
        try(function() map:SetAsync("k", 1, 10, true) end)
        try(function() map:SetAsync("k", string.rep("x", 40000), 10) end)
        try(function() map:GetRangeAsync(Enum.SortDirection.Ascending, 201) end)
        try(function() mss:GetQueue("Q"):ReadAsync(101) end)
        try(function() mss:GetHashMap("H"):ListItemsAsync(0) end)
        try(function() map:SetAsync("k", nil, 10) end)
        try(function() map:SetAsync("k", 1, 0 / 0) end)
        try(function() mss:GetQueue("Q"):AddAsync(1, math.huge) end)
        try(function() mss:GetHashMap("H"):SetAsync("k", 1, -math.huge) end)
        try(function() mss:GetQueue("Q", 0 / 0) end)
        try(function() mss:GetQueue("Q", math.huge) end)
        try(function() mss:GetQueue("Q"):ReadAsync(1, false, 0 / 0) end)
        try(function() mss:GetQueue("Q"):ReadAsync(1, false, math.huge) end)
        "#,
        0,
        &[
            "InvalidExpirationTime: The field 'expiration' time must be between 0 and 3,888,000.",
            "InvalidExpirationTime: The field 'expiration' time must be between 0 and 3,888,000.",
            "InvalidRequest: The key must be at most 128 characters.",
            "InvalidRequest: Unable to convert value to json.",
            "InvalidRequest: Unable to convert sortKey to a valid number or string.",
            "ItemValueSizeTooLarge: Value size exceeds limit (32 KB).",
            "InvalidRequest: The field 'count' must be between 1 and 200.",
            "InvalidRequest: The field 'count' must be between 1 and 100.",
            "InvalidRequest: The field 'count' must be between 1 and 200.",
            "Argument 2 missing or nil",
            "InvalidExpirationTime: The field 'expiration' time must be between 0 and 3,888,000.",
            "InvalidExpirationTime: The field 'expiration' time must be between 0 and 3,888,000.",
            "InvalidExpirationTime: The field 'expiration' time must be between 0 and 3,888,000.",
            "InvalidRequest: The field 'invisibilityTimeout' must be a finite number.",
            "InvalidRequest: The field 'invisibilityTimeout' must be a finite number.",
            "InvalidRequest: The field 'waitTimeout' must be a finite number.",
            "InvalidRequest: The field 'waitTimeout' must be a finite number.",
        ],
    );
}
