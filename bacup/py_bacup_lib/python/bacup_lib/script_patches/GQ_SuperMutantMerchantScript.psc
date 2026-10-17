; FO76 refills the PossibleDestination aliases for every leg; Fallout 4 has no RefillAlias. Workshops come from
; WorkshopParent instead, where Tales registers the Appalachia workshops and the player's C.A.M.P. The aliases,
; filled once when the quest starts, remain the fallback.
; Timer 1 ends a stop, timer 2 is the travel watchdog. Tales' globals FFF3B0 (stop length) and FFF3B1 (the
; running timer's deadline) are optional.

Event OnQuestInit()
    Depart()
EndEvent

Event OnTimerGameTime(Int aiTimerID)
    If aiTimerID == 1
        Depart()
    ElseIf aiTimerID == 2
        RecoverStalledLeg()
    EndIf
EndEvent

; Tales calls this after each load: a save that started the quest before the route existed never gets OnQuestInit.
Function ResumeRoute()
    Actor grahm = Merchant.GetActorReference()
    If !IsRunning() || grahm == None
        Return
    EndIf
    Float hoursLeft = -1.0
    GlobalVariable deadline = TalesGlobal(0x00FFF3B1)
    If deadline != None && deadline.GetValue() > 0.0
        hoursLeft = (deadline.GetValue() - Utility.GetCurrentGameTime()) * 24.0
    EndIf
    If CurrentDestination.GetReference() == None
        If hoursLeft > 0.0
            StartTimerGameTime(hoursLeft, 1)
        Else
            Depart()
        EndIf
    ElseIf hoursLeft > 0.0
        StartTimerGameTime(hoursLeft, 2)
        PlayRouteScene(grahm)
    Else
        RecoverStalledLeg()
    EndIf
EndFunction

Function Depart()
    Actor grahm = Merchant.GetActorReference()
    If grahm == None
        Return
    EndIf
    CancelTimerGameTime(1)
    CancelTimerGameTime(2)
    ObjectReference stop = PickNextStop(grahm)
    If stop == None
        CurrentDestination.Clear()
        ScheduleRouteTimer(StopHours(), 1)
        Return
    EndIf
    CurrentDestination.ForceRefTo(stop)
    ScheduleRouteTimer(8.0, 2)
    PlayRouteScene(grahm)
EndFunction

; The scene's walking phase completes within 2500 units of CurrentDestination and calls this at its end.
Function ArriveAtStop()
    Actor grahm = Merchant.GetActorReference()
    ObjectReference stop = CurrentDestination.GetReference()
    If grahm == None || stop == None || grahm.GetDistance(stop) > 2500.0
        Return
    EndIf
    CancelTimerGameTime(2)
    CurrentDestination.Clear()
    GQ_SuperMutantMerchant_Scene.Stop()
    ScheduleRouteTimer(StopHours(), 1)
    grahm.EvaluatePackage()
EndFunction

; Stage 1's note names this FO76 debug helper.
Function DebugMoveMerchantToWorkshop()
    Depart()
    Actor grahm = Merchant.GetActorReference()
    ObjectReference stop = CurrentDestination.GetReference()
    If grahm != None && stop != None
        TeleportToStop(grahm, stop)
    EndIf
EndFunction

Function RecoverStalledLeg()
    Actor grahm = Merchant.GetActorReference()
    If grahm == None
        Return
    EndIf
    ObjectReference stop = CurrentDestination.GetReference()
    If stop == None
        Depart()
        Return
    EndIf
    ; Never move him where the player can see it; give the walk another hour instead.
    If grahm.Is3DLoaded() || stop.Is3DLoaded()
        ScheduleRouteTimer(1.0, 2)
        PlayRouteScene(grahm)
        Return
    EndIf
    TeleportToStop(grahm, stop)
EndFunction

Function TeleportToStop(Actor akMerchant, ObjectReference akStop)
    akMerchant.MoveTo(akStop)
    ReferenceAlias brahmin = GetAlias(5) as ReferenceAlias
    If brahmin != None
        Actor chally = brahmin.GetActorReference()
        If chally != None && !chally.Is3DLoaded()
            chally.MoveTo(akStop, 200.0)
        EndIf
    EndIf
    ArriveAtStop()
EndFunction

Function PlayRouteScene(Actor akMerchant)
    If !GQ_SuperMutantMerchant_Scene.IsPlaying()
        GQ_SuperMutantMerchant_Scene.Start()
    EndIf
    akMerchant.EvaluatePackage()
EndFunction

; FO76's rolls run in order: a settlement, then a C.A.M.P., then an owned workshop, then an unowned one.
ObjectReference Function PickNextStop(Actor akMerchant)
    ObjectReference stop = None
    If Utility.RandomInt(1, 100) <= iChanceSettlement
        stop = UsableStop(PossibleDestination_SettlementCenterMarker.GetReference(), akMerchant)
    EndIf
    If stop == None && Utility.RandomInt(1, 100) <= iChanceCamp
        stop = RandomWorkshopStop(akMerchant, 2)
        If stop == None
            stop = UsableStop(PossibleDestination_Camp.GetReference(), akMerchant)
        EndIf
    EndIf
    If stop == None
        stop = RandomWorkshopStop(akMerchant, 1)
    EndIf
    If stop == None
        stop = RandomWorkshopStop(akMerchant, 0)
    EndIf
    If stop == None
        stop = UsableStop(PossibleDestination_WorkshopOwned.GetReference(), akMerchant)
    EndIf
    If stop == None
        stop = UsableStop(PossibleDestination_WorkshopUnowned.GetReference(), akMerchant)
    EndIf
    Return stop
EndFunction

; aiKind: 0 unowned workshop, 1 player-owned workshop, 2 C.A.M.P.
ObjectReference Function RandomWorkshopStop(Actor akMerchant, Int aiKind)
    WorkshopParentScript workshopParent = Game.GetFormFromFile(0x0002058E, "Fallout4.esm") as WorkshopParentScript
    If workshopParent == None
        Return None
    EndIf
    WorkshopScript[] workshops = workshopParent.Workshops
    If workshops == None
        Return None
    EndIf
    Keyword campKeyword = Game.GetFormFromFile(0x0005231A, "SeventySix.esm") as Keyword
    ObjectReference[] stops = new ObjectReference[0]
    Int i = 0
    While i < workshops.Length
        WorkshopScript workshop = workshops[i]
        If workshop != None
            Int kind = 0
            If campKeyword != None && workshop.HasKeyword(campKeyword)
                kind = 2
            ElseIf workshop.OwnedByPlayer
                kind = 1
            EndIf
            If kind == aiKind
                ObjectReference candidate = UsableStop(workshop, akMerchant)
                If candidate != None
                    stops.Add(candidate)
                EndIf
            EndIf
        EndIf
        i += 1
    EndWhile
    If stops.Length == 0
        Return None
    EndIf
    Return stops[Utility.RandomInt(0, stops.Length - 1)]
EndFunction

; FO76's searches keep the next stop in Grahm's worldspace and at least 15000 units away.
ObjectReference Function UsableStop(ObjectReference akCandidate, Actor akMerchant)
    If akCandidate == None || akCandidate.IsDisabled() || akCandidate.GetWorldSpace() != akMerchant.GetWorldSpace()
        Return None
    EndIf
    If akCandidate.GetDistance(akMerchant) < 15000.0
        Return None
    EndIf
    ObjectReference center = akCandidate.GetLinkedRef(WorkshopLinkCenter)
    If center != None
        Return center
    EndIf
    Return akCandidate
EndFunction

Function ScheduleRouteTimer(Float afHours, Int aiTimerID)
    StartTimerGameTime(afHours, aiTimerID)
    GlobalVariable deadline = TalesGlobal(0x00FFF3B1)
    If deadline != None
        deadline.SetValue(Utility.GetCurrentGameTime() + afHours / 24.0)
    EndIf
EndFunction

Float Function StopHours()
    GlobalVariable setting = TalesGlobal(0x00FFF3B0)
    If setting != None && setting.GetValue() > 0.0
        Return setting.GetValue()
    EndIf
    Return 1.0
EndFunction

GlobalVariable Function TalesGlobal(Int aiFormID)
    If !Game.IsPluginInstalled("B21_TalesFromAppalachia.esm")
        Return None
    EndIf
    Return Game.GetFormFromFile(aiFormID, "B21_TalesFromAppalachia.esm") as GlobalVariable
EndFunction
