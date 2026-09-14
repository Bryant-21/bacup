Function Fragment_Stage_0100_Item_00()
    If Alias_Player
        Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
    EndIf
    RegisterForQuestActors()
    UpdateToyProgress()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200)
    FillQuestObjectAliases()
    SetObjectiveDisplayed(300)
    If IsStageDone(475) && !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
    UpdateToyProgress()
EndFunction

Function Fragment_Stage_0420_Item_00()
    UpdateToyProgress()
EndFunction

Function Fragment_Stage_0430_Item_00()
    UpdateToyProgress()
EndFunction

Function Fragment_Stage_0440_Item_00()
    UpdateToyProgress()
EndFunction

Function Fragment_Stage_0450_Item_00()
    UpdateToyProgress()
EndFunction

Function Fragment_Stage_0475_Item_00()
    SetToyCountFromStages()
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetToyCountFromStages()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0610_Item_00()
    Actor player = Game.GetPlayer()
    If player && Moon_SQ01_Kieran_AV_KidsGotToys
        player.SetValue(Moon_SQ01_Kieran_AV_KidsGotToys, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(500)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    RemoveToy(Moon_SQ01_Toy_1)
    RemoveToy(Moon_SQ01_Toy_2)
    RemoveToy(Moon_SQ01_Toy_3)
    RemoveToy(Moon_SQ01_Toy_4)
    RemoveToy(Moon_SQ01_Toy_5)
    Stop()
EndFunction

Function RegisterForQuestActors()
    ReferenceAlias kieranAlias = GetAlias(38) as ReferenceAlias
    ReferenceAlias vinnyAlias = GetAlias(39) as ReferenceAlias
    If kieranAlias && kieranAlias.GetReference()
        RegisterForRemoteEvent(kieranAlias.GetReference(), "OnActivate")
    EndIf
    If vinnyAlias && vinnyAlias.GetReference()
        RegisterForRemoteEvent(vinnyAlias.GetReference(), "OnActivate")
    EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
    Actor player = Game.GetPlayer()
    Actor speaker = akSender as Actor
    If !player || akActivator != player || !speaker
        Return
    EndIf

    ReferenceAlias kieranAlias = GetAlias(38) as ReferenceAlias
    ReferenceAlias vinnyAlias = GetAlias(39) as ReferenceAlias
    Int nextStage = -1
    If vinnyAlias && akSender == vinnyAlias.GetReference() && IsStageDone(600) && !IsStageDone(700)
        nextStage = 700
    ElseIf kieranAlias && akSender == kieranAlias.GetReference() && IsStageDone(500) && !IsStageDone(600)
        nextStage = 600
    ElseIf kieranAlias && akSender == kieranAlias.GetReference() && IsStageDone(200) && !IsStageDone(300)
        nextStage = 300
    ElseIf vinnyAlias && akSender == vinnyAlias.GetReference() && IsStageDone(100) && !IsStageDone(200)
        nextStage = 200
    EndIf
    If nextStage < 0
        Return
    EndIf

    Int checks = 0
    While speaker.GetDialogueTarget() != player && checks < 40
        Utility.Wait(0.25)
        checks += 1
    EndWhile
    If speaker.GetDialogueTarget() != player
        Return
    EndIf

    checks = 0
    While speaker.GetDialogueTarget() == player && checks < 2400
        Utility.Wait(0.25)
        checks += 1
    EndWhile
    If speaker.GetDialogueTarget() == player
        Return
    EndIf

    If !IsStageDone(nextStage)
        SetStage(nextStage)
    EndIf
EndEvent

Function FillQuestObjectAliases()
    FillQuestObjectAlias(Alias_QuestObject_Toy_1, Alias_Dispenser_Truck_1)
    FillQuestObjectAlias(Alias_QuestObject_Toy_2, Alias_Dispenser_Truck_2)
    FillQuestObjectAlias(Alias_QuestObject_Toy_3, Alias_Dispenser_Truck_3)
    FillQuestObjectAlias(Alias_QuestObject_Toy_4, Alias_Dispenser_Truck_4)
    FillQuestObjectAlias(Alias_QuestObject_Toy_5, Alias_Dispenser_Truck_5)
EndFunction

Function FillQuestObjectAlias(ReferenceAlias questObjectAlias, ReferenceAlias dispenserAlias)
    If questObjectAlias && dispenserAlias && dispenserAlias.GetReference()
        questObjectAlias.ForceRefIfEmpty(dispenserAlias.GetReference())
    EndIf
EndFunction

Function UpdateToyProgress()
    SetToyCountFromStages()
    If IsStageDone(410) && IsStageDone(420) && IsStageDone(430) && IsStageDone(440) && IsStageDone(450)
        If IsStageDone(300) && !IsStageDone(500)
            SetStage(500)
        ElseIf !IsStageDone(300) && !IsStageDone(475)
            SetStage(475)
        EndIf
    EndIf
EndFunction

Function SetToyCountFromStages()
    Int count = 0
    If IsStageDone(410)
        count += 1
    EndIf
    If IsStageDone(420)
        count += 1
    EndIf
    If IsStageDone(430)
        count += 1
    EndIf
    If IsStageDone(440)
        count += 1
    EndIf
    If IsStageDone(450)
        count += 1
    EndIf

    Actor player = Game.GetPlayer()
    If player && Moon_SQ01_AV_ToysCurrent
        player.SetValue(Moon_SQ01_AV_ToysCurrent, count)
    EndIf
EndFunction

Function RemoveToy(Form toy)
    Actor player = Game.GetPlayer()
    If player && toy
        player.RemoveItem(toy, 1, True)
    EndIf
EndFunction
