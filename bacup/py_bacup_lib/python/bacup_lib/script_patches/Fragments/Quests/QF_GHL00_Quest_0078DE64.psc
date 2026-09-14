Function Fragment_Stage_0400_Item_00()
    If GHL00_02_GhoulCamp_AsherConfrontation != None && !GHL00_02_GhoulCamp_AsherConfrontation.IsPlaying()
        GHL00_02_GhoulCamp_AsherConfrontation.Start()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    If Scene_GHL00_08_GhoulCamp_PartheniaLeamonAmbient != None && !Scene_GHL00_08_GhoulCamp_PartheniaLeamonAmbient.IsPlaying()
        Scene_GHL00_08_GhoulCamp_PartheniaLeamonAmbient.Start()
    EndIf
EndFunction
