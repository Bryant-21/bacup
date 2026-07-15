Bool Function ShouldSpawnStatue()
    Location currentLocation = GetCurrentLocation()
    If currentLocation != None
        If LocNoAutoPlacedScorchedStatues != None && currentLocation.HasKeyword(LocNoAutoPlacedScorchedStatues)
            Return False
        EndIf

        Bool isScorchedLocation = (LocEncSubScorched != None && currentLocation.HasKeyword(LocEncSubScorched)) || (LocEncMainScorched != None && currentLocation.HasKeyword(LocEncMainScorched))
        If isScorchedLocation && (SubEncAmbush == None || currentLocation.HasRefType(SubEncAmbush))
            Return True
        EndIf
    EndIf

    If ScorchedStatueSpawnChance == None
        Return True
    EndIf
    Return Utility.RandomFloat(0.0, 100.0) <= ScorchedStatueSpawnChance.GetValue()
EndFunction

Function SpawnStatue()
    If !ShouldSpawnStatue() || MyScorchedStatue == None
        GoToState("notspawned")
        Return
    EndIf

    If LinkedScorchedStatue != None
        myStatueRef = GetLinkedRef(LinkedScorchedStatue)
    EndIf
    If myStatueRef == None
        myStatueRef = PlaceAtMe(MyScorchedStatue, 1, True, False, True)
        If myStatueRef != None && LinkedScorchedStatue != None
            SetLinkedRef(myStatueRef, LinkedScorchedStatue)
        EndIf
    EndIf

    If myStatueRef != None
        GoToState("spawned")
    Else
        GoToState("notspawned")
    EndIf
EndFunction

Function RemoveStatue()
    If myStatueRef == None && LinkedScorchedStatue != None
        myStatueRef = GetLinkedRef(LinkedScorchedStatue)
    EndIf
    If myStatueRef != None
        If ExplosionScorchDecal != None
            myStatueRef.PlaceAtMe(ExplosionScorchDecal)
        EndIf
        myStatueRef.Disable()
        myStatueRef.Delete()
        myStatueRef = None
    EndIf
    GoToState("notspawned")
EndFunction

Event OnInit()
    GoToState("waiting")
    SpawnStatue()
EndEvent

State waiting
    Event OnCellAttach()
        SpawnStatue()
    EndEvent
EndState

State spawned
    Event OnExitFurniture(ObjectReference akActionRef)
        RemoveStatue()
    EndEvent
EndState
