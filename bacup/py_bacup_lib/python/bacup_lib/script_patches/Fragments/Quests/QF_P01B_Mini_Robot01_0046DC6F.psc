Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && CaseStatusAV != None
        playerRef.SetValue(CaseStatusAV, 1.0)
    EndIf
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveDisplayed(110, True)
    ObjectReference crimeNotes = Alias_CrimeSceneNotes.GetReference()
    If crimeNotes != None
        crimeNotes.Enable(False)
    EndIf
    ObjectReference boPeepNote = Alias_BoPeepBlacksheepNote.GetReference()
    If boPeepNote != None
        boPeepNote.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_0151_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && P01B_Mini_Robot_CluesFoundLoc1 != None
        Float clueCount = 1.0
        If IsStageDone(152)
            clueCount += 1.0
        EndIf
        playerRef.SetValue(P01B_Mini_Robot_CluesFoundLoc1, clueCount)
        If clueCount >= MaxCluesLoc1
            SetObjectiveCompleted(110, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0152_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && P01B_Mini_Robot_CluesFoundLoc1 != None
        Float clueCount = 1.0
        If IsStageDone(151)
            clueCount += 1.0
        EndIf
        playerRef.SetValue(P01B_Mini_Robot_CluesFoundLoc1, clueCount)
        If clueCount >= MaxCluesLoc1
            SetObjectiveCompleted(110, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveDisplayed(310, True)
    ObjectReference wolfTape = Alias_Dispenser_WolfToBoPeepHolotape.GetReference()
    If wolfTape != None
        wolfTape.Enable(False)
    EndIf
    ObjectReference psychEval = Alias_Dispenser_CalvinPsychEval.GetReference()
    If psychEval != None
        psychEval.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_0251_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && P01B_Mini_Robot_CluesFoundLoc2 != None
        Float clueCount = 1.0
        If IsStageDone(252)
            clueCount += 1.0
        EndIf
        playerRef.SetValue(P01B_Mini_Robot_CluesFoundLoc2, clueCount)
        If clueCount >= MaxCluesLoc2
            SetObjectiveCompleted(310, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0252_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && P01B_Mini_Robot_CluesFoundLoc2 != None
        Float clueCount = 1.0
        If IsStageDone(251)
            clueCount += 1.0
        EndIf
        playerRef.SetValue(P01B_Mini_Robot_CluesFoundLoc2, clueCount)
        If clueCount >= MaxCluesLoc2
            SetObjectiveCompleted(310, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    SetObjectiveCompleted(300, True)
    SetObjectiveCompleted(310, True)
    SetObjectiveDisplayed(450, True)
    If playerRef != None && P01B_Mini_Robot_CluesFoundLoc3 != None
        playerRef.SetValue(P01B_Mini_Robot_CluesFoundLoc3, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(450, True)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    SetObjectiveCompleted(100, True)
    SetObjectiveCompleted(300, True)
    SetObjectiveCompleted(450, True)
    CompleteQuest()
    If playerRef != None && CaseStatusAV != None
        playerRef.SetValue(CaseStatusAV, 3.0)
    EndIf
    If MasterQuest != None
        Bool perfect = playerRef != None && playerRef.GetValue(P01B_Mini_Robot_CluesFoundLoc1) >= MaxCluesLoc1
        perfect = perfect && playerRef.GetValue(P01B_Mini_Robot_CluesFoundLoc2) >= MaxCluesLoc2
        perfect = perfect && playerRef.GetValue(P01B_Mini_Robot_CluesFoundLoc3) >= MaxCluesLoc3
        If perfect
            MasterQuest.SetStage(8010)
        Else
            MasterQuest.SetStage(8000)
        EndIf
    EndIf
    If playerRef != None && P01B_Lying_02_StartKeyword != None
        P01B_Lying_02_StartKeyword.SendStoryEventAndWait(None, playerRef)
    EndIf
EndFunction
