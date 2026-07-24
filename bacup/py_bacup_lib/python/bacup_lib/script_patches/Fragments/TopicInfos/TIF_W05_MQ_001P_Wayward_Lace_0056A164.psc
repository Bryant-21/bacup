Function Fragment_End(ObjectReference akSpeakerRef)
    If LL_Weapon_Ranged_PipeGun
        Game.GetPlayer().AddItem(LL_Weapon_Ranged_PipeGun, 1, False)
    EndIf
    If Ammo38Caliber
        Game.GetPlayer().AddItem(Ammo38Caliber, 1, False)
    EndIf
EndFunction
