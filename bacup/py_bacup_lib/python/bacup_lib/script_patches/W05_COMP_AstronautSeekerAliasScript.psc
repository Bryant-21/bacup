Event OnAliasInit()
    AssignLocalSeeker()
    ArmRefillCheck()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == CheckTimerID
        AssignLocalSeeker()
        ArmRefillCheck()
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(CheckTimerID)
EndEvent

Function AssignLocalSeeker()
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning()
        Return
    EndIf

    Actor currentSeeker = GetActorReference()
    If currentSeeker != None && !currentSeeker.IsDead()
        Return
    EndIf
    If SpawnEnemies == None
        Return
    EndIf

    Int enemyIndex = 0
    While enemyIndex < SpawnEnemies.GetCount()
        Actor enemy = SpawnEnemies.GetAt(enemyIndex) as Actor
        If enemy != None && !enemy.IsDead()
            ForceRefTo(enemy)
            enemy.EvaluatePackage()
            Return
        EndIf
        enemyIndex += 1
    EndWhile
EndFunction

Function ArmRefillCheck()
    Quest owner = GetOwningQuest()
    If owner != None && owner.IsRunning() && RefillCheck > 0
        CancelTimer(CheckTimerID)
        StartTimer(RefillCheck as Float, CheckTimerID)
    EndIf
EndFunction
