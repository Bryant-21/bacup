; Free-play offline loop. Token registration, multiplayer ownership and objectives
; remain server-only; bound target forms/nodes drive play. Points pay out on game end.

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If gameActive
        LocalGameEnd()
    Else
        myPlayer = akActionRef as Actor
        gameActive = True
        score = 0
        PlayStartSFX()
        PlayMainLoopSFX()
        LocalGameStart()
        If gameDuration != None
            StartTimer(gameDuration.GetValue(), 90)
        EndIf
    EndIf
EndEvent

Function LocalGameStart()
    stopSpawns = False
    moveSpeed = StartMoveSpeed
    TimeBetweenSpawns = StartTimeBetweenSpawns
    If TimeBetweenSpawns <= 0.0
        TimeBetweenSpawns = 1.0
    EndIf
    SpawnNextTarget()
EndFunction

Function SpawnNextTarget()
    If stopSpawns || TargetForms == None || TargetForms.Length == 0
        Return
    EndIf
    Int targetIndex = Utility.RandomInt(0, TargetForms.Length - 1)
    String spawnNode = LRSpawnNode
    If targetIndex >= TargetForms.Length / 2
        spawnNode = RLSpawnNode
    EndIf
    ArcadeShootingGalleryTarget target = PlaceAtNode(spawnNode, TargetForms[targetIndex], 1, True, False, True, True) as ArcadeShootingGalleryTarget
    If target != None
        target.GameController = Self
        target.MoveAnimation(moveSpeed)
    EndIf
    StartTimer(TimeBetweenSpawns, 91)
EndFunction

Function RegisterTargetHit(Int targetScore)
    score = score + targetScore
    moveSpeed = Math.Min(moveSpeed + MoveSpeedIncreaseOnHit, MaxMoveSpeed)
EndFunction

Function RegisterTargetMiss()
    moveSpeed = Math.Min(moveSpeed + MoveSpeedIncreaseOnMiss, MaxMoveSpeed)
    TimeBetweenSpawns = Math.Max(TimeBetweenSpawns - timeBetweenSpawnsDecreaseOnMiss, MinTimeBetweenSpawns)
EndFunction

Function LocalGameEnd()
    If !gameActive
        Return
    EndIf
    stopSpawns = True
    gameActive = False
    CancelTimer(91)
    CancelTimer(90)
    PayoutPoints()
    StopMainLoopSFX()
    PlayEndSFX()
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 90
        LocalGameEnd()
    ElseIf aiTimerID == 91 && gameActive
        SpawnNextTarget()
    EndIf
EndEvent

; Same payout as ArcadeWhackAMole.PayoutPoints; the shared parent skeleton cannot carry it.
Function PayoutPoints()
    Actor playerRef = Game.GetPlayer()
    If myPlayer != playerRef || score <= 0
        Return
    EndIf
    If ticketsOut == None && TicketSlot != None && TicketNodes != None && TicketNodes.Length > 0
        ticketsOut = PlaceAtNode(TicketNodes[0], TicketSlot, 1, False, False, False, True) as ArcadeTicketDispenser
    EndIf
    If ticketsOut != None
        ticketsOut.scorePerTicket = scorePerTicket
        ticketsOut.PayoutPoints(playerRef, score)
    EndIf
EndFunction
