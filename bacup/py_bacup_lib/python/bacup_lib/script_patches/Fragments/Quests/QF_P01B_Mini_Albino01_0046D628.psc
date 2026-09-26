Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && CaseStatusAV != None
        playerRef.SetValue(CaseStatusAV, 1.0)
    EndIf
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    If BarrelMessage != None
        BarrelMessage.Show()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If JarMessage != None
        JarMessage.Show()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    If AerosolizerMessage != None
        AerosolizerMessage.Show()
    EndIf
    SetObjectiveDisplayed(31, True)
    ObjectReference testKit = Alias_Dispenser_TestKit.GetReference()
    If testKit != None
        testKit.Enable(False)
    EndIf
    P01B_Albino_TrySampleStage()
EndFunction

Function Fragment_Stage_0510_Item_00()
    SetObjectiveCompleted(31, True)
    If IsStageDone(500)
        SetObjectiveDisplayed(32, True)
    EndIf
    P01B_Albino_TrySampleStage()
EndFunction

; 520 (kit in hand and the aerosolizer entry read) gates the aerosolizer (530) but had no
; setter; either half can arrive last.
Function P01B_Albino_TrySampleStage()
    If IsStageDone(500) && IsStageDone(510) && !IsStageDone(520)
        SetStage(520)
    EndIf
EndFunction

; The confession note alias has no fill (FO76 created it server-side); place it at its
; bound marker so the alias OnRead (800, preReq 700) can fire.
Function P01B_Albino_PlaceConfession()
    If Alias_ConfessionNote == None || Alias_ConfessionNote.GetReference() != None || P01B_Mini_Albino01_Confession == None
        Return
    EndIf
    ObjectReference markerRef = None
    If Alias_ConfessionNoteLocation != None
        markerRef = Alias_ConfessionNoteLocation.GetReference()
    EndIf
    If markerRef == None
        markerRef = p01b_mini_albino_confessionnotelocation
    EndIf
    If markerRef == None
        Return
    EndIf
    ObjectReference noteRef = markerRef.PlaceAtMe(P01B_Mini_Albino01_Confession, 1, True)
    If noteRef != None
        Alias_ConfessionNote.ForceRefTo(noteRef)
    EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
    SetObjectiveCompleted(31, True)
    SetObjectiveDisplayed(32, True)
EndFunction

Function Fragment_Stage_0530_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && P01B_MiscItem_ChemicalSample != None
        playerRef.AddItem(P01B_MiscItem_ChemicalSample, 1, False)
    EndIf
    SetObjectiveCompleted(32, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveCompleted(40, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(50, True)
    If !IsStageDone(675)
        SetStage(675)
    EndIf
EndFunction

Function Fragment_Stage_0675_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(50, True)
    P01B_Albino_PlaceConfession()
    ObjectReference confession = Alias_ConfessionNote.GetReference()
    If confession != None
        confession.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(50, True)
    P01B_Albino_PlaceConfession()
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(50, True)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    SetObjectiveCompleted(10, True)
    SetObjectiveCompleted(31, True)
    SetObjectiveCompleted(32, True)
    SetObjectiveCompleted(40, True)
    SetObjectiveCompleted(50, True)
    CompleteQuest()
    If playerRef != None
        If P01B_Mini_Albino01_Completed != None
            playerRef.SetValue(P01B_Mini_Albino01_Completed, 1.0)
        EndIf
        If CaseStatusAV != None
            playerRef.SetValue(CaseStatusAV, 3.0)
        EndIf
    EndIf
    If MasterQuest != None
        If IsStageDone(250) && IsStageDone(300) && IsStageDone(400) && IsStageDone(550) && IsStageDone(600) && IsStageDone(650) && IsStageDone(800)
            MasterQuest.SetStage(7010)
        Else
            MasterQuest.SetStage(7000)
        EndIf
    EndIf
EndFunction
