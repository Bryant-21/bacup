Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && pBoSz03RegisteredForRecon != None && playerRef.GetValue(pBoSz03RegisteredForRecon) < 1.0
        If pBoS02_LL_Weapon_Ranged_BOS_Sniper_Rifle != None
            playerRef.AddItem(pBoS02_LL_Weapon_Ranged_BOS_Sniper_Rifle, 1, False)
        EndIf
        If pBoSZ03_recipe_ReconRifle != None
            playerRef.AddItem(pBoSZ03_recipe_ReconRifle, 1, False)
        EndIf
        If pBoSZ03_Recipe_ReconRifleRecipe != None
            playerRef.AddItem(pBoSZ03_Recipe_ReconRifleRecipe, 1, False)
        EndIf
        playerRef.SetValue(pBoSz03RegisteredForRecon, 1.0)
    EndIf
EndFunction
