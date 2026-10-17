Function Fragment_Stage_0010_Item_00()
    Game.GetPlayer().SetValue(ATX_COMP_AV_Ghoul_CompletedIntroduction, 1.0)
EndFunction

Function Fragment_Stage_0020_Item_00()
    Game.GetPlayer().SetValue(ATX_COMP_AV_Ghoul_PlayerAskedAboutServices, 1.0)
EndFunction

Function Fragment_Stage_0030_Item_00()
    Game.GetPlayer().SetValue(ATX_COMP_AV_Ghoul_PlayerAskedAboutSpaceFlight, 1.0)
EndFunction

; Stage 40 is set by the TIQS of INFO 62CB3E ("...the Zebulonian Medal of Honor. Here.").
Function Fragment_Stage_0040_Item_00()
    Game.GetPlayer().SetValue(ATX_COMP_AV_Ghoul_GaveMedalOfHonor, 1.0)
    ATX_COMP_GhoulScript ghoul = (Self as Quest) as ATX_COMP_GhoulScript
    If ghoul
        ghoul.GiveMedalOfHonor()
    EndIf
EndFunction
