Function Fragment_Stage_0010_Item_00()
    Actor player = None
    If Alias_currentPlayer != None
        player = Alias_currentPlayer.GetActorReference()
    EndIf
    If player == None
        player = Game.GetPlayer()
        If player != None && Alias_currentPlayer != None
            Alias_currentPlayer.ForceRefTo(player)
        EndIf
    EndIf

    If player != None && EN05_Basic_MiscStartedValue != None
        player.SetValue(EN05_Basic_MiscStartedValue, 1.0)
    EndIf
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(10)
    If Alias_currentPlayer != None
        Alias_currentPlayer.Clear()
    EndIf
    Stop()
EndFunction
