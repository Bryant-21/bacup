Function Fragment_Stage_0010_Item_00()
    Game.GetPlayer().SetValue(AV_Intro, 1.0)
EndFunction

Function Fragment_Stage_0020_Item_00()
    Game.GetPlayer().ModValue(ATX_COMP_AV_SuperMutantAffinity, -1.0)
EndFunction

Function Fragment_Stage_0030_Item_00()
    Game.GetPlayer().ModValue(ATX_COMP_AV_SuperMutantAffinity, 1.0)
EndFunction

Function Fragment_Stage_0040_Item_00()
    Game.GetPlayer().SetValue(ATX_COMP_AV_SuperMutant_Variable04_SAVED, 1.0)
EndFunction

Function Fragment_Stage_0100_Item_00()
    Game.GetPlayer().SetValue(ATX_COMP_AV_SuperMutant_Variable02_SAVED, 1.0)
EndFunction

Function Fragment_Stage_0200_Item_00()
    Game.GetPlayer().SetValue(ATX_COMP_AV_SuperMutant_Variable03_SAVED, 1.0)
EndFunction
