Function Fragment_Stage_0100_Item_00()
    Actor player = Game.GetPlayer()
    If Alias_Player && player
        Alias_Player.ForceRefIfEmpty(player)
    EndIf
    SetObjectiveDisplayed(10)
    SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveCompleted(15)
    SetObjectiveDisplayed(17)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(17)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveCompleted(15)
    SetObjectiveCompleted(17)
    SetObjectiveDisplayed(18)
EndFunction

Function Fragment_Stage_0350_Item_00()
    ObjectReference colossusRef = Alias_Colossus.GetReference()
    If colossusRef && Object_PocketWatch && colossusRef.GetItemCount(Object_PocketWatch) < 1
        colossusRef.AddItem(Object_PocketWatch, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(18)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0700_Item_00()
    Actor player = Game.GetPlayer()
    If player && E06_PlayerResponseToMaggieWilliams
        player.SetValue(E06_PlayerResponseToMaggieWilliams, 1.0)
    EndIf
    SetObjectiveCompleted(30)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0740_Item_00()
    Actor player = Game.GetPlayer()
    If player
        If E06_PlayerResponseToMaggieWilliams
            player.SetValue(E06_PlayerResponseToMaggieWilliams, 2.0)
        EndIf
        ObjectReference pocketWatchRef = Alias_PocketWatch.GetReference()
        If pocketWatchRef && pocketWatchRef.GetContainer() == player
            player.RemoveItem(pocketWatchRef, 1, True)
        ElseIf Object_PocketWatch
            player.RemoveItem(Object_PocketWatch, 1, True)
        EndIf
    EndIf
    SetObjectiveCompleted(30)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0760_Item_00()
    Actor player = Game.GetPlayer()
    If player
        If E06_PlayerResponseToMaggieWilliams
            player.SetValue(E06_PlayerResponseToMaggieWilliams, 3.0)
        EndIf
        ObjectReference pocketWatchRef = Alias_PocketWatch.GetReference()
        If pocketWatchRef && pocketWatchRef.GetContainer() == player
            player.RemoveItem(pocketWatchRef, 1, True)
        ElseIf Object_PocketWatch
            player.RemoveItem(Object_PocketWatch, 1, True)
        EndIf
    EndIf
    SetObjectiveCompleted(30)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction
