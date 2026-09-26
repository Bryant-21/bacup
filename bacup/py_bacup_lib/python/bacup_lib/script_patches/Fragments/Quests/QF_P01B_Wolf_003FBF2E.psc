Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

; The card reader alias sets 105 unconditionally; 110 is the "no keycard yet" branch.
Function Fragment_Stage_0105_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && GarrahanEstateAccessKeycard != None && playerRef.GetItemCount(GarrahanEstateAccessKeycard) == 0
        SetStage(110)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    If !IsStageDone(125)
        If !IsStageDone(120)
            SetObjectiveDisplayed(11, True)
        EndIf
        SetObjectiveDisplayed(12, True)
    EndIf
EndFunction

; The note can be read before the reader is tried (its alias only requires stage 100).
Function Fragment_Stage_0120_Item_00()
    SetObjectiveCompleted(11, True)
    If !IsStageDone(125) && !IsStageDone(200)
        SetObjectiveDisplayed(12, True)
    EndIf
EndFunction

Function Fragment_Stage_0125_Item_00()
    SetObjectiveCompleted(12, True)
    If !IsObjectiveCompleted(11)
        SetObjectiveDisplayed(11, False)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    If !IsObjectiveCompleted(11)
        SetObjectiveDisplayed(11, False)
    EndIf
    If !IsObjectiveCompleted(12)
        SetObjectiveDisplayed(12, False)
    EndIf
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && P01B_Wolf_RecallKey != None
        playerRef.AddItem(P01B_Wolf_RecallKey, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveCompleted(30, True)
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && P01B_Wolf_RecallKey != None && playerRef.GetItemCount(P01B_Wolf_RecallKey) == 0
        playerRef.AddItem(P01B_Wolf_RecallKey, 1, False)
    EndIf
    If QuestP01B_Master != None
        QuestP01B_Master.SetStage(9500)
    EndIf
    If P01B_Wolf_Misc_StartKeyword != None && playerRef != None
        P01B_Wolf_Misc_StartKeyword.SendStoryEvent(None, playerRef)
    EndIf
EndFunction
