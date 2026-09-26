Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None || BrokenCameraObject == None || IsStageDone(200)
        Return
    EndIf
    If BrokenCamera.GetReference() == None && playerRef.GetItemCount(BrokenCameraObject) == 0
        ObjectReference cameraRef = playerRef.PlaceAtMe(BrokenCameraObject, 1, True)
        If cameraRef != None
            BrokenCamera.ForceRefTo(cameraRef)
            playerRef.AddItem(cameraRef, 1, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    If !IsStageDone(1500)
        SetObjectiveDisplayed(100, True)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    If !IsStageDone(1600)
        SetObjectiveDisplayed(110, True)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    If !IsStageDone(1700)
        SetObjectiveDisplayed(120, True)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    If !IsStageDone(1800)
        SetObjectiveDisplayed(130, True)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    If !IsStageDone(1900)
        SetObjectiveDisplayed(140, True)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    If !IsStageDone(2000)
        SetObjectiveDisplayed(150, True)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    If !IsStageDone(2100)
        SetObjectiveDisplayed(160, True)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(110, True)
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(120, True)
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveCompleted(130, True)
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveCompleted(140, True)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveCompleted(150, True)
EndFunction

Function Fragment_Stage_2100_Item_00()
    SetObjectiveCompleted(160, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(20, True)
    Stop()
EndFunction
