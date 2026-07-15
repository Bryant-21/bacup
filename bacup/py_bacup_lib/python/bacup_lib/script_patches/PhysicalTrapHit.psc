; Offline FO4 physical-hit behavior using only the trap's bound payload and the
; native ProcessTrapHit contract.

Event OnLoad()
    If damage <= 0
        damage = 25
    EndIf
    If soundLevel <= 0
        soundLevel = 10
    EndIf
    SetCanHit(CanHitAtStart)
EndEvent

Function SetCanHit(Bool TrapCanHit)
    If TrapCanHit
        GoToState("canhit")
    Else
        GoToState("CannotHit")
    EndIf
EndFunction

State canhit
    Event OnTrapHitStart(ObjectReference akTarget, Float afXVel, Float afYVel, Float afZVel, Float afXPos, Float afYPos, Float afZPos, Int aeMaterial, Bool abInitialHit, Int aeMotionType)
        If akTarget == None
            Return
        EndIf

        akTarget.ProcessTrapHit(Self, damage as Float, afPushBack, afXVel, afYVel, afZVel, afXPos, afYPos, afZPos, aeMaterial, aiStaggerAmount as Float)

        If DiseaseSpell != None
            DiseaseSpell.Cast(Self, akTarget)
        EndIf
        If hitFX != None
            hitFX.Fire(Self)
        EndIf
        Actor targetActor = akTarget as Actor
        If targetActor != None && soundLevel > 0
            CreateDetectionEvent(targetActor, soundLevel)
        EndIf
        If SelfDamageOnHit > 0.0
            DamageObject(SelfDamageOnHit)
        EndIf
        If akTarget == Game.GetPlayer()
            RumbleAndCameraShake()
        EndIf
    EndEvent
EndState

Function cRMIRumbleAndCameraShake(Float fContRumbleAmplitude, Float fContRumbleDuration, Float fCamShakeAmplitude, Float fCamShakeDuration)
    If fContRumbleAmplitude > 0.0
        Game.ShakeController(fContRumbleAmplitude, fContRumbleAmplitude, fContRumbleDuration)
    EndIf
    If fCamShakeAmplitude > 0.0
        Game.ShakeCamera(None, fCamShakeAmplitude, fCamShakeDuration)
    EndIf
EndFunction

Function RumbleAndCameraShake()
    cRMIRumbleAndCameraShake(fControllerRumbleAmplitude, fControllerRumbleDuration, fCameraShakeAmplitude, fCameraShakeDuration)
EndFunction
