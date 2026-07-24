Event OnLoad()
    If MTNM01_Mayhem.IsRunning()
        If !myHazard
            myHazard = PlaceAtMe(MTNM01_BaitMineScentAttractorMeatHazard)
        EndIf
        StartTimer(ExplosionCheckTimerSeconds, 1)
    EndIf
EndEvent

; Must cancel on unload - FO4 has no RegisterForSingleUpdate/OnUpdate (Skyrim-only,
; confirmed absent from the FO4 Papyrus API), so this timer is the only recheck
; loop, and letting it keep firing after the ref unloads would be an orphaned
; polling loop bloating the save.
Event OnUnload()
    CancelTimer(1)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1
        Return
    EndIf

    ; Quest-gate failure terminates the loop - no reschedule once the timed event
    ; quest has stopped, so no live hazard/explosive is left behind outside it.
    If !MTNM01_Mayhem.IsRunning()
        Return
    EndIf

    ObjectReference[] nearby = FindAllReferencesOfType(MTNM01_BaitMine_ActorKeywordList, ExplosionCheckRadius)
    Int i = 0
    Bool triggered = False
    While i < nearby.Length && !triggered
        Actor creature = nearby[i] as Actor
        If creature && !creature.IsDead()
            triggered = True
        EndIf
        i += 1
    EndWhile

    If triggered
        ; Detonation is terminal - no reschedule.
        PlaceAtMe(MTNM01_Expl_BaitMine)
        If myHazard
            myHazard.Delete()
            myHazard = None
        EndIf
    Else
        StartTimer(ExplosionCheckTimerSeconds, 1)
    EndIf
EndEvent
