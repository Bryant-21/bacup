Event OnAliasInit()
    NumClonesAlive = 0
    B21BossVulnerable = False
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    EvaluateClones()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 4690
        EvaluateClones()
    EndIf
EndEvent

Function BeginCloneTracking()
    B21BossVulnerable = False
    EvaluateClones()
EndFunction

Function StopCloneTracking()
    CancelTimer(4690)
EndFunction

Int Function CountLivingClones()
    Int living = 0
    Int index = 0
    Int cloneCount = GetCount()
    While index < cloneCount
        Actor clone = GetAt(index) as Actor
        If clone != None && !clone.IsDead()
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

; The monster stays invulnerable while any clone lives or the clone wave is still spawning.
Function EvaluateClones()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning() || !owningQuest.IsStageDone(900) || owningQuest.IsStageDone(9000) || owningQuest.IsStageDone(9990)
        StopCloneTracking()
        Return
    EndIf
    NumClonesAlive = CountLivingClones()
    DefaultQuestEncounterWaveScript ews = owningQuest as DefaultQuestEncounterWaveScript
    Bool clonesPending = False
    If ews != None
        Int cloneWave = ews.FindEncounterWaveIndex("Flatwoods Monster Clones")
        clonesPending = cloneWave >= 0 && ews.IsEncounterWaveSpawning(cloneWave)
    EndIf
    Bool vulnerable = NumClonesAlive <= 0 && !clonesPending
    Quests:E01C_Tales:Dark:FlatwoodsBossScript bossScript = None
    If Alias_Actors_FlatwoodsBoss != None
        bossScript = Alias_Actors_FlatwoodsBoss as Quests:E01C_Tales:Dark:FlatwoodsBossScript
    EndIf
    If bossScript != None
        bossScript.ApplyVulnerability(vulnerable)
    EndIf
    If vulnerable && !B21BossVulnerable
        owningQuest.SetObjectiveCompleted(90, True)
        owningQuest.SetObjectiveDisplayed(100, True, True)
    EndIf
    B21BossVulnerable = vulnerable
    CancelTimer(4690)
    If !vulnerable || bossScript == None || bossScript.GetCount() == 0
        StartTimer(2.0, 4690)
    EndIf
EndFunction
