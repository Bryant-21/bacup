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
    gameActive = False
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
