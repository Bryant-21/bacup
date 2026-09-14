Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveCompleted(20)
    If Alias_TunnelIntroDoor
        ObjectReference introDoor = Alias_TunnelIntroDoor.GetReference()
        If introDoor
            introDoor.Lock(False)
            introDoor.SetOpen(True)
        EndIf
    EndIf
    SetObjectiveDisplayed(30)
    If E09C_LoveTunnel_PA_StartDecorations && !E09C_LoveTunnel_PA_StartDecorations.IsPlaying()
        E09C_LoveTunnel_PA_StartDecorations.Start()
    EndIf
EndFunction

Function Fragment_Stage_0190_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(35)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0290_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(45)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0350_Item_00()
    If Alias_MrHandy && E09C_MrLovelyMoveToWeddingMarker
        ObjectReference mrLovely = Alias_MrHandy.GetReference()
        If mrLovely
            mrLovely.MoveTo(E09C_MrLovelyMoveToWeddingMarker)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0390_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(65)
    SetObjectiveDisplayed(70)
    If E09C_WeddingDialogueEnabled
        E09C_WeddingDialogueEnabled.SetValue(1.0)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(70)
    SetStage(9000)
EndFunction

Function Fragment_Stage_9900_Item_00()
    ; DefaultSetStageOnQuestTimerEnd drives this stage when the event clock runs out.
    ; A run that already reached the wedding payoff must not be converted into a failure.
    If !GetStageDone(500) && !GetStageDone(9000)
        SetStage(9990)
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
    If E09C_WeddingDialogueEnabled
        E09C_WeddingDialogueEnabled.SetValue(0.0)
    EndIf
EndFunction
