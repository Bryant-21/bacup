; The population sign counts players inside LocationToCount; in single player that is 1 while the player is there.
Event OnLoad()
    UpdateLocationCount()
    Parent.OnLoad()
EndEvent

Function UpdateLocationCount()
    Count = 0
    If CountPlayers && LocationToCount != None && Game.GetPlayer().IsInLocation(LocationToCount)
        Count = 1
    EndIf
EndFunction
