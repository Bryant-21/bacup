Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    SetStage(399)
EndFunction

Function Fragment_Stage_0399_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(30, True)
    If playerRef != None && AV_ProgressCurrent != None
        playerRef.SetValue(AV_ProgressCurrent, 0.0)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 9.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0401_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 18.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0402_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 27.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0403_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 36.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0404_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 45.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0405_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 54.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0406_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 63.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0407_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 72.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0408_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 81.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0409_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 90.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 100.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
    SetObjectiveCompleted(20, True)
    SetObjectiveCompleted(40, True)
    SetObjectiveDisplayed(50, True)
    SetObjectiveCompleted(50, True)
    SetStage(9000)
EndFunction

Function Fragment_Stage_8999_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_HasFailed != None
        playerRef.SetValue(AV_HasFailed, 1.0)
    EndIf
    SetObjectiveFailed(20, True)
    If Quest_M01C_Athletics != None && !Quest_M01C_Athletics.IsStageDone(800)
        Quest_M01C_Athletics.SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_HasFailed != None
        playerRef.SetValue(AV_HasFailed, 2.0)
    EndIf
    SetObjectiveCompleted(20, True)
    SetObjectiveCompleted(30, True)
    SetObjectiveCompleted(40, True)
    SetObjectiveCompleted(50, True)
    CompleteQuest()
    If Quest_M01C_Athletics != None && !Quest_M01C_Athletics.IsStageDone(900)
        Quest_M01C_Athletics.SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_HasFailed != None
        playerRef.SetValue(AV_HasFailed, 1.0)
    EndIf
    SetObjectiveFailed(20, True)
    If Quest_M01C_Athletics != None && !Quest_M01C_Athletics.IsStageDone(9000)
        Quest_M01C_Athletics.SetStage(9000)
    EndIf
EndFunction
