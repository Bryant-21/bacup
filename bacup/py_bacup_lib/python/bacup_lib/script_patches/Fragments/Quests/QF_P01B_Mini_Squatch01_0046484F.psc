Function Fragment_Stage_0090_Item_00()
    SetObjectiveDisplayed(10, True)
    P01B_Squatch01_StockDiary()
EndFunction

; Cindy's diary alias has no fill (FO76 created it in the tent server-side), so its OnRead
; (600) could never fire and 800/9000 were unreachable. Create it in the bound tent container.
Function P01B_Squatch01_StockDiary()
    If Alias_QO_Book_Diary == None || Alias_QO_Book_Diary.GetReference() != None || P01B_Mini_Squatch01_Diary == None
        Return
    EndIf
    ObjectReference tentRef = Alias_Container_Tent.GetReference()
    If tentRef == None
        Return
    EndIf
    ObjectReference diaryRef = tentRef.PlaceAtMe(P01B_Mini_Squatch01_Diary, 1, True)
    If diaryRef != None
        tentRef.AddItem(diaryRef, 1, True)
        Alias_QO_Book_Diary.ForceRefTo(diaryRef)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    P01B_Squatch01_StockDiary()
    ObjectReference campTape = Alias_Dispenser_MainHolotape.GetReference()
    If campTape != None
        campTape.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    If IsStageDone(700) && !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    If IsStageDone(600) && !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
    ObjectReference meatTape = Alias_Dispenser_MeatHolotape.GetReference()
    If meatTape != None
        meatTape.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveCompleted(20, True)
    CompleteQuest()
    Quest masterQuest = Game.GetFormFromFile(0x0047F444, "SeventySix.esm") as Quest
    If masterQuest != None
        If IsStageDone(100) && IsStageDone(600) && IsStageDone(700) && IsStageDone(900)
            masterQuest.SetStage(4010)
        Else
            masterQuest.SetStage(4000)
        EndIf
    EndIf
EndFunction
