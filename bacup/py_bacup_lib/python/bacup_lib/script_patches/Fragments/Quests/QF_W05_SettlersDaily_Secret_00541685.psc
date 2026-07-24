Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0199_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True, True)
EndFunction

Function Fragment_Stage_0201_Item_00()
    SetObjectiveDisplayed(201, True, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
    If Alias_currentPlayer != None && Alias_currentPlayer.GetActorReference() != None
        Alias_currentPlayer.GetActorReference().SetValue(W05_SettlersDaily_Secret01, 1.0)
    EndIf
    SetObjectiveCompleted(201, True)
EndFunction

Function Fragment_Stage_0299_Item_00()
    SetObjectiveCompleted(200, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction
