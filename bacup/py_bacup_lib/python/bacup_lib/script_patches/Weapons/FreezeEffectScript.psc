Function RemoveCurrentFreezeSpell()
    If victim != None && currentSpell != None
        victim.RemoveSpell(currentSpell)
    EndIf
    currentSpell = None
EndFunction

Function SetFreezeStage(Int stageIndex)
    If victim == None || FreezeStageData == None || stageIndex < 0 || stageIndex >= FreezeStageData.Length
        Return
    EndIf

    RemoveCurrentFreezeSpell()
    currentStage = stageIndex
    currentSpell = FreezeStageData[currentStage].FreezeSpell
    If currentSpell != None
        victim.AddSpell(currentSpell, False)
    EndIf

    If currentStage == 0
        GoToState("normal")
    ElseIf currentStage == 1
        GoToState("chilled")
    ElseIf currentStage == 2
        GoToState("frosted")
    Else
        GoToState("frozen")
    EndIf
EndFunction

Function ResetFreezeProgress()
    CancelTimer(hitTimerID)
    currentNumberOfHits = 0
    If victim != None
        SetFreezeStage(0)
    EndIf
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
    victim = akTarget
    currentShader = None
    currentSpell = None
    currentStage = 0
    currentNumberOfHits = 0
    If victim != None && FreezeStageData != None && FreezeStageData.Length > 0
        SetFreezeStage(0)
    EndIf
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    If victim == None || akTarget != victim || akSource != CryolatorWeapon
        Return
    EndIf
    If Health != None && victim.GetValue(Health) <= 0.0
        Return
    EndIf

    currentNumberOfHits += 1
    CancelTimer(hitTimerID)
    If LastHitTime > 0.0
        StartTimer(LastHitTime, hitTimerID)
    EndIf

    If FreezeStageData != None && currentStage < 3 && currentStage < FreezeStageData.Length
        Int requiredHits = FreezeStageData[currentStage].NumberOfHits
        If requiredHits > 0 && currentNumberOfHits >= requiredHits
            SetFreezeStage(currentStage + 1)
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == hitTimerID
        ResetFreezeProgress()
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    CancelTimer(hitTimerID)
    RemoveCurrentFreezeSpell()
    If victim != None && currentShader != None
        currentShader.Stop(victim)
    EndIf
    currentShader = None
    victim = None
    currentStage = 0
    currentNumberOfHits = 0
    GoToState("")
EndEvent
