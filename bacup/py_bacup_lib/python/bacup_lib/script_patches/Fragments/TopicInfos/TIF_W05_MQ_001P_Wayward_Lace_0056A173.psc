Function Fragment_End(ObjectReference akSpeakerRef)
    If CharGenLL_Weapon_Simple_Melee_Machete_FullHealth
        Game.GetPlayer().AddItem(CharGenLL_Weapon_Simple_Melee_Machete_FullHealth, 1, False)
    EndIf
    If W05_MQ_001P_Wayward_LaceyIsela_PlayerGotGun
        Game.GetPlayer().SetValue(W05_MQ_001P_Wayward_LaceyIsela_PlayerGotGun, 1.0)
    EndIf
EndFunction
