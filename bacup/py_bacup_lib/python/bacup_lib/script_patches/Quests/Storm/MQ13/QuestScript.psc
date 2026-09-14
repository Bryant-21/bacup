Quests:Storm:MQ13:LostGauntletScript Function WaveController()
    Return (Self as Quest) as Quests:Storm:MQ13:LostGauntletScript
EndFunction

Function BeginBossAddPhase()
    ClearEncounterCollection(Alias_HugoClones)
    Quests:Storm:MQ13:LostGauntletScript controller = WaveController()
    If controller != None
        controller.StartGauntletWave(2)
    EndIf
    StartElectricArcs()
EndFunction

Function BeginClonePhase()
    ClearEncounterCollection(Alias_LostMobs)
    Quests:Storm:MQ13:LostGauntletScript controller = WaveController()
    If controller != None
        controller.StartGauntletWave(3)
    EndIf
EndFunction

Function BeginFinalSoloPhase()
    ClearEncounterCollection(Alias_LostMobs)
    ClearEncounterCollection(Alias_HugoClones)
EndFunction

Function EndBossFight()
    CancelTimer(iElectricArcTimerID)
    ClearEncounterCollection(Alias_LostMobs)
    ClearEncounterCollection(Alias_HugoClones)
EndFunction

Function ClearEncounterCollection(RefCollectionAlias akCollection)
    If akCollection == None
        Return
    EndIf

    Int index = 0
    While index < akCollection.GetCount()
        Actor encounterActor = akCollection.GetAt(index) as Actor
        If encounterActor != None
            encounterActor.StopCombat()
            encounterActor.DisableNoWait()
        EndIf
        index += 1
    EndWhile
    akCollection.RemoveAll()
EndFunction

Function StartElectricArcs()
    CancelTimer(iElectricArcTimerID)
    If IsStageDone(500) && !IsStageDone(600)
        StartTimer(fTimeBetweenArcs, iElectricArcTimerID)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != iElectricArcTimerID || !IsStageDone(500) || IsStageDone(600)
        Return
    EndIf

    ObjectReference player = Alias_Player.GetReference()
    Int index = 0
    While player != None && Alias_ElectricArcSource != None && index < Alias_ElectricArcSource.GetCount()
        Quests:Storm:MQ13:ElectricalArcScript arcSource = Alias_ElectricArcSource.GetAt(index) as Quests:Storm:MQ13:ElectricalArcScript
        If arcSource != None
            arcSource.FireAtRef_Client(player)
        EndIf
        index += 1
    EndWhile
    StartTimer(fTimeBetweenArcs, iElectricArcTimerID)
EndEvent

Function CaptureHugo()
    If !IsStageDone(iCaptureHugoStage)
        Return
    EndIf

    Actor player = Alias_Player.GetActorReference()
    Actor hugo = Alias_Hugo.GetActorReference()
    If player != None
        FadeToBlackSpell.Cast(player, player)
    EndIf
    If hugo == None
        Return
    EndIf

    hugo.StopCombat()
    ObjectReference captiveFurnitureRef = hugo.GetLinkedRef(LinkCustom01)
    If captiveFurnitureRef == None
        captiveFurnitureRef = hugo.PlaceAtMe(CaptiveFurniture, 1, False, True)
        If captiveFurnitureRef != None
            hugo.SetLinkedRef(captiveFurnitureRef, LinkCustom01)
        EndIf
    EndIf
    If captiveFurnitureRef != None
        captiveFurnitureRef.EnableNoWait()
        captiveFurnitureRef.Activate(hugo)
    EndIf
EndFunction

Function ShutdownFinale()
    EndBossFight()
    Actor hugo = Alias_Hugo.GetActorReference()
    If hugo != None
        hugo.StopCombat()
        ObjectReference captiveFurnitureRef = hugo.GetLinkedRef(LinkCustom01)
        If captiveFurnitureRef != None
            captiveFurnitureRef.DisableNoWait()
            captiveFurnitureRef.Delete()
            hugo.SetLinkedRef(None, LinkCustom01)
        EndIf
    EndIf
EndFunction

Event OnQuestShutdown()
    ShutdownFinale()
EndEvent
