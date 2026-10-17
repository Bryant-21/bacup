; Free-play single-player loop using the fully bound attached-ref mole table.

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
    If Moles == None || Moles.Length == 0
        Return
    EndIf
    Targets = new ArcadeWhackAMoleTarget[Moles.Length]
    MolesActive = 0
    MoleCap = 1
    Int i = 0
    While i < Moles.Length
        ArcadeWhackAMoleTarget target = PlaceAtNode(Moles[i].NodeName, Moles[i].FormToPlace, 1, True, False, True, True) as ArcadeWhackAMoleTarget
        Targets[i] = target
        If target != None
            target.gameController = Self
            StartMoles(target)
        EndIf
        i = i + 1
    EndWhile
EndFunction

Function RegisterMoleHit(Int hitScore)
    score = score + hitScore
EndFunction

Function LocalGameEnd()
    If !gameActive
        Return
    EndIf
    gameActive = False
    CancelTimer(90)
    PayoutPoints()
    StopMainLoopSFX()
    PlayEndSFX()
    Int i = 0
    While Targets != None && i < Targets.Length
        If Targets[i] != None
            Targets[i].StopGame()
            Targets[i].Delete()
        EndIf
        i = i + 1
    EndWhile
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 90
        LocalGameEnd()
    EndIf
EndEvent

; FO76 paid out through the machine's ticket peripheral (TicketSlot at TicketNodes[0]), which owns
; the points AV, per-game cap and first-payout tutorial. Lives on the child because the generated
; ArcadeController skeleton does not compile (its GameEnd custom event declaration was lost).
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
