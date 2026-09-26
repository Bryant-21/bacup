Int Function HealObjectiveFor(ObjectReference akMember)
    If akMember == None
        Return -1
    ElseIf BoSRifleman != None && BoSRifleman.GetReference() == akMember
        Return 34
    ElseIf BoSScout != None && BoSScout.GetReference() == akMember
        Return 35
    ElseIf BoSTechnician != None && BoSTechnician.GetReference() == akMember
        Return 36
    EndIf
    Return -1
EndFunction

Message Function DownMessageFor(Int aiObjective)
    If aiObjective == 34
        Return Rifleman_Down_Message
    ElseIf aiObjective == 35
        Return Scout_Down_Message
    EndIf
    Return Technician_Down_Message
EndFunction

Bool Function ArenaFightActive(Quest akQuest)
    If akQuest == None || !akQuest.IsRunning() || !akQuest.IsStageDone(300)
        Return False
    EndIf
    Return !akQuest.IsStageDone(900) && !akQuest.IsStageDone(9000) && !akQuest.IsStageDone(9991) && !akQuest.IsStageDone(9992) && !akQuest.IsStageDone(9999)
EndFunction

Bool Function IsHealObjectiveOpen(Quest akQuest, Int aiObjective)
    Return akQuest.IsObjectiveDisplayed(aiObjective) && !akQuest.IsObjectiveCompleted(aiObjective) && !akQuest.IsObjectiveFailed(aiObjective)
EndFunction

Event OnEnterBleedout(ObjectReference akSenderRef)
    Quest owningQuest = GetOwningQuest()
    Int objective = HealObjectiveFor(akSenderRef)
    If objective < 0 || !ArenaFightActive(owningQuest)
        Return
    EndIf
    ; Heal objectives 34-36 map to their death stages 901-903.
    If owningQuest.IsStageDone(objective + 867)
        Return
    EndIf
    Message downMessage = DownMessageFor(objective)
    If downMessage != None
        downMessage.Show()
    EndIf
    ; B21:ObjectiveTimers restarts the bleedout countdown whenever the heal objective is shown again.
    owningQuest.SetObjectiveDisplayed(objective, True, True)
    StartTimer(1.0, 1)
EndEvent

Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    Actor member = akSenderRef as Actor
    If playerRef == None || akActionRef != playerRef || member == None || member.IsDead() || !member.IsBleedingOut()
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    Int objective = HealObjectiveFor(akSenderRef)
    If objective < 0 || !ArenaFightActive(owningQuest) || !IsHealObjectiveOpen(owningQuest, objective)
        Return
    EndIf
    ; FO76 players revive a downed gladiator with a stimpak; FO4 has no ally-revive action, so activation spends one.
    Potion stimpak = Game.GetFormFromFile(0x00023736, "Fallout4.esm") as Potion
    If stimpak == None || playerRef.GetItemCount(stimpak) < 1
        Return
    EndIf
    owningQuest.SetObjectiveDisplayed(objective, False)
    playerRef.RemoveItem(stimpak, 1, False)
    member.ResetHealthAndLimbs()
    member.EvaluatePackage()
EndEvent

Bool Function ReconcileHealObjective(Quest akQuest, ReferenceAlias akMemberAlias, Int aiObjective)
    If !IsHealObjectiveOpen(akQuest, aiObjective)
        Return False
    EndIf
    Actor member = None
    If akMemberAlias != None
        member = akMemberAlias.GetActorReference()
    EndIf
    If member != None && !member.IsDead() && !member.IsBleedingOut()
        akQuest.SetObjectiveDisplayed(aiObjective, False)
        Return False
    EndIf
    Return True
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If !ArenaFightActive(owningQuest)
        Return
    EndIf
    Bool anyOpen = ReconcileHealObjective(owningQuest, BoSRifleman, 34)
    If ReconcileHealObjective(owningQuest, BoSScout, 35)
        anyOpen = True
    EndIf
    If ReconcileHealObjective(owningQuest, BoSTechnician, 36)
        anyOpen = True
    EndIf
    If anyOpen
        StartTimer(1.0, 1)
    EndIf
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    Int objective = HealObjectiveFor(akSenderRef)
    Quest owningQuest = GetOwningQuest()
    If objective < 0 || owningQuest == None || !owningQuest.IsRunning()
        Return
    EndIf
    If owningQuest.IsStageDone(9000) || owningQuest.IsStageDone(9991) || owningQuest.IsStageDone(9992) || owningQuest.IsStageDone(9999)
        Return
    EndIf
    ; The converted team aliases carry no Essential flag, so a gladiator can die without a heal countdown.
    Int deathStage = objective + 867
    If !owningQuest.IsStageDone(deathStage)
        owningQuest.SetStage(deathStage)
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(1)
EndEvent
