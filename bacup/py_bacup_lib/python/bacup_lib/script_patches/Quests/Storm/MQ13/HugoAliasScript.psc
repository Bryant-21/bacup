Event OnAliasInit()
    MQ13QuestScript = GetOwningQuest() as Quests:Storm:MQ13:QuestScript
    myActor = GetActorReference()
    iCurrentFightStage = 0
EndEvent

Function BeginFinalFight()
    If MQ13QuestScript == None
        MQ13QuestScript = GetOwningQuest() as Quests:Storm:MQ13:QuestScript
    EndIf
    If myActor == None
        myActor = GetActorReference()
    EndIf
    If myActor == None || MQ13QuestScript == None || MQ13QuestScript.IsStageDone(600)
        Return
    EndIf

    iCurrentFightStage = 0
    myActor.SetEssential(True)
    myActor.SetValue(Storm_MQ13_HugoFightState, 1.0)
    ApplyFightStageLoadout(0)
    Actor player = Alias_Player.GetActorReference()
    If player != None
        myActor.StartCombat(player)
    EndIf
EndFunction

Event OnEnterBleedout()
    If myActor == None
        myActor = GetActorReference()
    EndIf
    If myActor == None || MQ13QuestScript == None || MQ13QuestScript.IsStageDone(600)
        Return
    EndIf

    iCurrentFightStage += 1
    If iFightStageArray != None && iCurrentFightStage - 1 < iFightStageArray.Length
        Int stageToSet = iFightStageArray[iCurrentFightStage - 1]
        If stageToSet > 0 && !MQ13QuestScript.IsStageDone(stageToSet)
            MQ13QuestScript.SetStage(stageToSet)
        EndIf
    EndIf

    myActor.StopCombat()
    If iCurrentFightStage >= iFightStages
        myActor.SetValue(Storm_MQ13_HugoFightState, 0.0)
        Return
    EndIf

    myActor.SetValue(Storm_MQ13_HugoFightState, 2.0)
    Storm_MQ13_HugoStealth.Cast(myActor, myActor)
    If HugoRechargeFurniture != None
        RegisterForAnimationEvent(myActor, sRechargeAnimationCompleteEvent)
        ObjectReference rechargeRef = Game.FindClosestReferenceOfTypeFromRef(HugoRechargeFurniture, myActor, fTeleportRadius)
        If rechargeRef != None
            myActor.SnapIntoInteraction(rechargeRef)
        EndIf
    EndIf
    CancelTimer(iHugoAnimationFailsafeTimerID)
    StartTimer(fRechargeAnimationLength, iHugoAnimationFailsafeTimerID)
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
    If akSource == myActor && asEventName == sRechargeAnimationCompleteEvent
        FinishRecharge()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iHugoAnimationFailsafeTimerID
        FinishRecharge()
    EndIf
EndEvent

Function FinishRecharge()
    If myActor == None || myActor.GetValue(Storm_MQ13_HugoFightState) != 2.0
        Return
    EndIf

    CancelTimer(iHugoAnimationFailsafeTimerID)
    UnregisterForAnimationEvent(myActor, sRechargeAnimationCompleteEvent)
    Actor player = Alias_Player.GetActorReference()
    If player != None
        Storm_MQ13_FlashbangSpell.Cast(myActor, player)
    EndIf
    myActor.DispelSpell(Storm_MQ13_HugoStealth)
    myActor.ResetHealthAndLimbs()
    ApplyFightStageLoadout(iCurrentFightStage)
    If Alias_SpawnCentre.GetReference() != None
        myActor.MoveTo(Alias_SpawnCentre.GetReference())
    EndIf
    myActor.SetValue(Storm_MQ13_HugoFightState, 1.0)
    If player != None
        myActor.StartCombat(player)
    EndIf
EndFunction

Event OnAliasShutdown()
    CancelTimer(iHugoAnimationFailsafeTimerID)
    If myActor != None
        UnregisterForAnimationEvent(myActor, sRechargeAnimationCompleteEvent)
    EndIf
EndEvent

Function ApplyFightStageLoadout(Int aiFightStage)
    If myActor == None
        Return
    EndIf
    If FightStageOutfits != None && aiFightStage >= 0 && aiFightStage < FightStageOutfits.Length && FightStageOutfits[aiFightStage] != None
        myActor.SetOutfit(FightStageOutfits[aiFightStage])
    EndIf
    If FightStageWeapons != None && aiFightStage >= 0 && aiFightStage < FightStageWeapons.Length && FightStageWeapons[aiFightStage] != None
        myActor.EquipItem(FightStageWeapons[aiFightStage], True, True)
    EndIf
EndFunction
