Function Fragment_Stage_0100_Item_00()
    If W05_RE_TravelBB01_NotInCombatScene != None
        W05_RE_TravelBB01_NotInCombatScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    If W05_RE_TravelBB01_InCombatScene != None
        W05_RE_TravelBB01_InCombatScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    If W05_RE_TravelBB01_SearchingScene != None
        W05_RE_TravelBB01_SearchingScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If DeadScene != None
        DeadScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction
