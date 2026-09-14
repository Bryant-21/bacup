Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    SetStage(299)
EndFunction

Function Fragment_Stage_0299_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(30, True)
    If playerRef != None && AV_ProgressCurrent != None
        playerRef.SetValue(AV_ProgressCurrent, 0.0)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 6.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0301_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 12.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0302_Item_00()
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

Function Fragment_Stage_0303_Item_00()
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

Function Fragment_Stage_0304_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 30.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0305_Item_00()
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

Function Fragment_Stage_0306_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 42.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0307_Item_00()
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

Function Fragment_Stage_0308_Item_00()
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

Function Fragment_Stage_0309_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 60.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 66.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0311_Item_00()
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

Function Fragment_Stage_0312_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 78.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0313_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If AV_ProgressCurrent != None
            playerRef.SetValue(AV_ProgressCurrent, 84.0)
        EndIf
        If Sound_CP != None
            Sound_CP.Play(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0314_Item_00()
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

Function Fragment_Stage_0315_Item_00()
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
