--!strict
-- The sandbox gamemode's player rules over the server's hooks (the game's gamemodes/sandbox):
--   sbox_godmode 1       nobody takes damage (GM:PlayerShouldTakeDamage)
--   sbox_weapons 0       the loadout is camera, tool gun and physgun only (GM:PlayerLoadout)
-- sbox_maxprops lives in limits.luau; sbox_noclip, sbox_playershurtplayers and mp_falldamage
-- are applied by the server itself.
state.blocked_damage = state.blocked_damage or 0
state.bare_loadouts = state.bare_loadouts or 0

events.on("player.damage", function(e)
    if cvar.get_bool("sbox_godmode") then
        state.blocked_damage += 1
        return false
    end
    return nil
end)

events.on("player.loadout", function(e)
    if cvar.get_bool("sbox_weapons") then
        return nil
    end
    state.bare_loadouts += 1
    for _, class in ipairs({ "gmod_camera", "gmod_tool", "weapon_physgun" }) do
        world.queue("player.give_weapon", e.player, class)
    end
    return false
end)
