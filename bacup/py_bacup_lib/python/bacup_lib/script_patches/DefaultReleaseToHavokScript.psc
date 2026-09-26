Function ReleaseHavok()
    If GetState() != "released"
        GoToState("released")
    EndIf
EndFunction

Event OnContainerChanged(ObjectReference akNewContainer, ObjectReference akOldContainer)
    ReleaseHavok()
EndEvent

Event OnUnload()
    UnregisterForAllHitEvents()
EndEvent

Event OnReset()
    GoToState("Initial")
    If Is3DLoaded()
        OnLoad()
    EndIf
EndEvent

State Initial
    Event OnLoad()
        SetMotionType(Motion_Keyframed, True)
        If HavokOnHit
            RegisterForHitEvent(Self)
        EndIf
    EndEvent

    Event OnActivate(ObjectReference akActionRef)
        If HavokOnActivate
            ReleaseHavok()
        EndIf
    EndEvent

    Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String asMaterialName)
        If HavokOnHit && akTarget == Self
            ReleaseHavok()
        EndIf
    EndEvent
EndState

State released
    Event OnBeginState(String asOldState)
        UnregisterForAllHitEvents()
        SetMotionType(Motion_Dynamic, True)
        If LinkHavokPartner != None
            DefaultReleaseToHavokScript partner = GetLinkedRef(LinkHavokPartner) as DefaultReleaseToHavokScript
            If partner != None && partner != Self
                partner.ReleaseHavok()
            EndIf
        EndIf
    EndEvent

    Event OnLoad()
        SetMotionType(Motion_Dynamic, True)
    EndEvent
EndState
