; Replace the remote win callback with a local linked-controller callback.

Event OnLoad()
    gameController = GetLinkedRef() as ArcadeNukaZapperRace
EndEvent

Function RegisterWin(Actor akSendingPlayer)
    If gameController != None && raceActive
        raceActive = False
        gameController.score = gameController.score + 1
        gameController.PlayWinSFX()
        gameController.EndRace(Self)
    EndIf
EndFunction

Function SendRMIToServer(String functionName, Var[] arguments)
    RegisterWin(Game.GetPlayer())
EndFunction
