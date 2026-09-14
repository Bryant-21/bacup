Function Fragment_Stage_0100_Item_00()
    If MineshaftMapMarker != None
        MineshaftMapMarker.AddToMap()
    EndIf
    SetObjectiveDisplayed(5)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(5)
    SetObjectiveDisplayed(10)
    SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(15)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(20)
EndFunction

Function Fragment_Stage_1000_Item_00()
    ObjectReference player = Alias_Player.GetReference()
    ObjectReference holotapeRef = Alias_Holotape.GetReference()
    If player != None
        If HandedInOneAV != None
            player.SetValue(HandedInOneAV, 1.0)
        EndIf
        If holotapeRef != None && holotapeRef.GetContainer() == player
            player.RemoveItem(holotapeRef, 1, True)
        EndIf
    EndIf
    SetObjectiveCompleted(20)
EndFunction

Function Fragment_Stage_1001_Item_00()
    ObjectReference player = Alias_Player.GetReference()
    ObjectReference holotapeRef = Alias_Holotape.GetReference()
    ObjectReference suppliesRef = Alias_Supplies.GetReference()
    If player != None
        If HandedInBothAV != None
            player.SetValue(HandedInBothAV, 1.0)
        EndIf
        If holotapeRef != None && holotapeRef.GetContainer() == player
            player.RemoveItem(holotapeRef, 1, True)
        EndIf
        If suppliesRef != None && suppliesRef.GetContainer() == player
            player.RemoveItem(suppliesRef, 1, True)
        EndIf
    EndIf
    SetObjectiveCompleted(20)
EndFunction

Function Fragment_Stage_1002_Item_00()
    ObjectReference player = Alias_Player.GetReference()
    ObjectReference suppliesRef = Alias_Supplies.GetReference()
    If player != None
        If HandedInOneAV != None
            player.SetValue(HandedInOneAV, 1.0)
        EndIf
        If suppliesRef != None && suppliesRef.GetContainer() == player
            player.RemoveItem(suppliesRef, 1, True)
        EndIf
    EndIf
    SetObjectiveCompleted(20)
EndFunction

Function Fragment_Stage_1003_Item_00()
    ObjectReference player = Alias_Player.GetReference()
    If player != None && HandedInNoneAV != None
        player.SetValue(HandedInNoneAV, 1.0)
    EndIf
    SetObjectiveCompleted(20)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(20)
EndFunction
