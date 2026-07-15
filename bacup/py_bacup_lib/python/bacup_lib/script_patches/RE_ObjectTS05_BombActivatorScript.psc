Event OnActivate(ObjectReference akActionRef)
    If GetState() == "inuse" || GetState() == "defused" || GetState() == "detonate"
        Return
    EndIf

    Actor activatingActor = akActionRef as Actor
    If activatingActor == None
        Return
    EndIf

    ActivatingPlayer = activatingActor
    GoToState("inuse")
    Int selectedWire = 0
    If RE_ObjectTS05_Dialogue != None
        selectedWire = RE_ObjectTS05_Dialogue.Show()
    EndIf
    If selectedWire < 0 || selectedWire > 2
        GoToState("notinuse")
        Return
    EndIf

    Float defuseRoll = Utility.RandomFloat(0.0, 100.0)
    If Intelligence != None
        defuseRoll += activatingActor.GetValue(Intelligence) * IntelligenceModifier
    EndIf
    If Luck != None
        defuseRoll += activatingActor.GetValue(Luck) * LuckModifier
    EndIf

    If defuseRoll >= RollSuccessThreshhold
        GoToState("defused")
        If RE_ObjectTS05_BombContainer != None
            PlaceAtMe(RE_ObjectTS05_BombContainer)
        EndIf
        Disable()
    Else
        GoToState("detonate")
        DamageObject(1000.0)
    EndIf
EndEvent
