Event OnEffectStart(Actor akTarget, Actor akCaster)
    canPlayOnCombatStartSound = True
    canPlayOnBossKillSound = True
    StartTimer(OnEnterSoundDelay as Float)
EndEvent

Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
    If aeCombatState == 1 && canPlayOnCombatStartSound
        canPlayOnCombatStartSound = False
        OnCombatStartSound.Play(GetTargetActor())
        StartTimer(OnCombatStartSoundCooldown as Float, onCombatStartSoundTimerID)
    EndIf
EndEvent

Event OnKill(Actor akVictim)
    If akVictim != None && canPlayOnBossKillSound && BossRaceList.HasForm(akVictim.GetRace())
        canPlayOnBossKillSound = False
        OnBossKillSound.Play(GetTargetActor())
        StartTimer(OnBossKillSoundCooldown as Float, onBossKillSoundTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0
        OnEnterSound.Play(GetTargetActor())
    ElseIf aiTimerID == onCombatStartSoundTimerID
        canPlayOnCombatStartSound = True
    ElseIf aiTimerID == onBossKillSoundTimerID
        canPlayOnBossKillSound = True
    EndIf
EndEvent
