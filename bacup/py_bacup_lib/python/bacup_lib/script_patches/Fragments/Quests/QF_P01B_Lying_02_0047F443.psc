Function Fragment_Stage_0001_Item_00()
    If !IsStageDone(325)
        SetObjectiveCompleted(20, True)
        SetObjectiveDisplayed(30, True)
    EndIf
EndFunction

Function Fragment_Stage_0009_Item_00()
    UpdateClueCount()
EndFunction

Function Fragment_Stage_0010_Item_00()
    UpdateClueCount()
EndFunction

Function Fragment_Stage_0011_Item_00()
    SetObjectiveCompleted(110, True)
    If IsObjectiveDisplayed(120)
        SetObjectiveCompleted(120, True)
    EndIf
    If !IsStageDone(1100)
        SetObjectiveDisplayed(130, True)
    EndIf
EndFunction

Function Fragment_Stage_0012_Item_00()
    If IsStageDone(13) && !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0013_Item_00()
    If IsStageDone(12) && !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    If !IsStageDone(325)
        SetObjectiveDisplayed(20, True)
    EndIf
    SetStage(250)
EndFunction

Function Fragment_Stage_0250_Item_00()
    ObjectReference bagRef = Alias_Container_PoliceCarBag.GetReference()
    If bagRef != None && holo_PoliceHoloPart1 != None && Alias_holo_PoliceHoloPart1.GetReference() == None
        ObjectReference holotapeRef = bagRef.PlaceAtMe(holo_PoliceHoloPart1, 1, True)
        If holotapeRef != None
            Alias_holo_PoliceHoloPart1.ForceRefTo(holotapeRef)
            bagRef.AddItem(holotapeRef, 1, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0325_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveCompleted(30, True)
    If !IsStageDone(400)
        SetObjectiveDisplayed(40, True)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(40, True)
    If !IsStageDone(500)
        SetObjectiveDisplayed(50, True)
    EndIf
    Actor playerRef = Alias_Player.GetActorReference()
    If !IsStageDone(425) && playerRef != None && holo_PoliceHoloPart2 != None && playerRef.GetItemCount(holo_PoliceHoloPart2) == 0
        SetObjectiveDisplayed(1900, True)
    EndIf
    ObjectReference coolerRef = Alias_container_BBQCooler.GetReference()
    If coolerRef != None && note_DeadDropReport != None && Alias_note_DeadDropReport.GetReference() == None
        ObjectReference noteRef = coolerRef.PlaceAtMe(note_DeadDropReport, 1, True)
        If noteRef != None
            Alias_note_DeadDropReport.ForceRefTo(noteRef)
            coolerRef.AddItem(noteRef, 1, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0425_Item_00()
    SetObjectiveCompleted(1900, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveCompleted(60, True)
    If !IsStageDone(600)
        SetObjectiveDisplayed(70, True)
    EndIf
    SetStage(550)
EndFunction

Function Fragment_Stage_0550_Item_00()
    ObjectReference noteRef = Alias_note_BoPeepNote.GetReference()
    If noteRef != None
        noteRef.Enable(False)
    EndIf
    ObjectReference keycardRef = Alias_key_Keycard.GetReference()
    If keycardRef != None
        keycardRef.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(70, True)
    If !IsStageDone(700)
        SetObjectiveDisplayed(80, True)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(80, True)
    If !IsStageDone(800)
        SetObjectiveDisplayed(90, True)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(90, True)
    If !IsStageDone(900)
        SetObjectiveDisplayed(100, True)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(100, True)
    If !IsStageDone(11)
        SetObjectiveDisplayed(110, True)
    EndIf
    If !IsStageDone(7000)
        SetObjectiveDisplayed(2000, True)
    EndIf
    ObjectReference lockerRef = Alias_container_PasswordLocker.GetReference()
    If lockerRef != None && key_TerminalPassword != None && Alias_key_TerminalPassword.GetReference() == None
        ObjectReference passwordRef = lockerRef.PlaceAtMe(key_TerminalPassword, 1, True)
        If passwordRef != None
            Alias_key_TerminalPassword.ForceRefTo(passwordRef)
            lockerRef.AddItem(passwordRef, 1, True)
        EndIf
    EndIf
    SetStage(950)
EndFunction

Function Fragment_Stage_0950_Item_00()
    ObjectReference passwordRef = Alias_key_TerminalPassword.GetReference()
    If passwordRef != None
        passwordRef.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    If !IsStageDone(11)
        SetObjectiveDisplayed(120, True)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(130, True)
    If !IsStageDone(1200)
        SetObjectiveDisplayed(140, True)
    EndIf
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && holo_CalvinRecording != None && Alias_holo_CalvinRecording.GetReference() == None && playerRef.GetItemCount(holo_CalvinRecording) == 0
        ObjectReference holotapeRef = playerRef.PlaceAtMe(holo_CalvinRecording, 1, True)
        If holotapeRef != None
            Alias_holo_CalvinRecording.ForceRefTo(holotapeRef)
            playerRef.AddItem(holotapeRef, 1, False)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1150_Item_00()
    If IsObjectiveDisplayed(150)
        SetObjectiveCompleted(150, True)
    EndIf
    If IsStageDone(1200)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(140, True)
    If IsStageDone(1150)
        SetStage(1300)
    Else
        SetObjectiveDisplayed(150, True)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If StartKeyword_WISC != None && playerRef != None
        StartKeyword_WISC.SendStoryEvent(None, playerRef)
    EndIf
    If QuestP01B_Master != None
        If IsStageDone(7000)
            QuestP01B_Master.SetStage(1010)
        Else
            QuestP01B_Master.SetStage(1000)
        EndIf
    EndIf
    SetStage(9000)
EndFunction

Function Fragment_Stage_7000_Item_00()
    SetObjectiveCompleted(2000, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    If IsObjectiveDisplayed(150)
        SetObjectiveCompleted(150, True)
    EndIf
    If !IsObjectiveCompleted(1900)
        SetObjectiveDisplayed(1900, False)
    EndIf
    If !IsObjectiveCompleted(2000)
        SetObjectiveDisplayed(2000, False)
    EndIf
EndFunction

Function UpdateClueCount()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None || AV_CluesCurrent == None
        Return
    EndIf
    Int found = 0
    Int clueStage = 8
    While clueStage <= 10
        If IsStageDone(clueStage)
            found += 1
        EndIf
        clueStage += 1
    EndWhile
    playerRef.SetValue(AV_CluesCurrent, found as Float)
EndFunction
