Function Fragment_Stage_0090_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
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
