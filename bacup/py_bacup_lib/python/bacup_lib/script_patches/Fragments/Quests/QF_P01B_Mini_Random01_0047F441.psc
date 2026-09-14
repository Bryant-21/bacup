Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If CaseStatusAV != None
            playerRef.SetValue(CaseStatusAV, 1.0)
        EndIf
        If AV_CluesCurrent != None
            playerRef.SetValue(AV_CluesCurrent, 0.0)
        EndIf
    EndIf
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
    SetObjectiveDisplayed(900, True)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(900, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0425_Item_00()
    SetObjectiveCompleted(40, True)
    SetObjectiveDisplayed(45, True)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(40, True)
    SetObjectiveDisplayed(45, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(45, True)
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(60, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    SetObjectiveCompleted(70, True)
    SetObjectiveDisplayed(90, True)
    If playerRef != None
        If AV_CluesCurrent != None && global_Clues_Max != None
            playerRef.SetValue(AV_CluesCurrent, global_Clues_Max.GetValue())
        EndIf
        If CaseStatusAV != None
            playerRef.SetValue(CaseStatusAV, 2.0)
        EndIf
    EndIf
    If !IsStageDone(725)
        SetStage(725)
    EndIf
EndFunction

Function Fragment_Stage_0725_Item_00()
    Actor creatureRef = Alias_mob_Creature.GetReference() as Actor
    Actor playerRef = Alias_Player.GetReference() as Actor
    If creatureRef != None
        creatureRef.Enable(False)
        If AmbushRelease != None
            creatureRef.SetValue(AmbushRelease, 1.0)
        EndIf
        If playerRef != None && !creatureRef.IsDead()
            creatureRef.StartCombat(playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    SetObjectiveCompleted(90, True)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_7000_Item_00()
    SetObjectiveCompleted(1000, True)
EndFunction

Function Fragment_Stage_7001_Item_00()
    ObjectReference journalOne = alias_note_Journal1.GetReference()
    If journalOne != None
        journalOne.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_7002_Item_00()
    ObjectReference journalTwo = alias_note_Journal2.GetReference()
    If journalTwo != None
        journalTwo.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    SetObjectiveCompleted(10, True)
    SetObjectiveCompleted(20, True)
    SetObjectiveCompleted(30, True)
    SetObjectiveCompleted(40, True)
    SetObjectiveCompleted(45, True)
    SetObjectiveCompleted(50, True)
    SetObjectiveCompleted(60, True)
    SetObjectiveCompleted(70, True)
    SetObjectiveCompleted(90, True)
    CompleteQuest()
    If playerRef != None && CaseStatusAV != None
        playerRef.SetValue(CaseStatusAV, 3.0)
    EndIf
    If MasterQuest != None
        If IsStageDone(7000)
            MasterQuest.SetStage(2010)
        Else
            MasterQuest.SetStage(2000)
        EndIf
    EndIf
EndFunction
