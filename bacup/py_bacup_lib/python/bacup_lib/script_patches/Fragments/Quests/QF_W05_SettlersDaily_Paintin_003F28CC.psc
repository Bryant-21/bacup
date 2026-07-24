Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0101_Item_00()
    SetObjectiveDisplayed(101, True, True)
EndFunction

Function Fragment_Stage_0102_Item_00()
    SetObjectiveCompleted(101, True)
EndFunction

Function Fragment_Stage_0110_Item_00()
    If Alias_currentPlayer != None && Alias_currentPlayer.GetActorReference() != None
        Alias_currentPlayer.GetActorReference().SetValue(W05_SettlersDaily_Graffiti01, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    If Alias_currentPlayer != None && Alias_currentPlayer.GetActorReference() != None
        Alias_currentPlayer.GetActorReference().SetValue(W05_SettlersDaily_Graffiti02, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0130_Item_00()
    If Alias_currentPlayer != None && Alias_currentPlayer.GetActorReference() != None
        Alias_currentPlayer.GetActorReference().SetValue(W05_SettlersDaily_Graffiti03, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0199_Item_00()
    SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction
