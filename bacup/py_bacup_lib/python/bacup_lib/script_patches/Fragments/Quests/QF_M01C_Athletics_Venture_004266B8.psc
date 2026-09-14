Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    SetStage(199)
EndFunction

Function Fragment_Stage_0199_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(30, True)
    If playerRef != None && AV_ProgressCurrent != None
        playerRef.SetValue(AV_ProgressCurrent, 0.0)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 8.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0201_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 16.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0202_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 24.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0203_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 32.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0204_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 40.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0205_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 48.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0206_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 56.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0207_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 64.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0208_Item_00()
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

Function Fragment_Stage_0209_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 80.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0210_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 88.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0211_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 96.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0212_Item_00()
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
