; The Drifter's smoke-screen teleport: its TeleportEnterExplosion animation event bursts it out in smoke and cloaks it
; with TeleportChameleonSpell; TeleportExitExplosion bursts it back in and drops the cloak.
Event OnEffectStart(Actor akTarget, Actor akCaster)
    selfRef = akTarget
    If selfRef == None
        Return
    EndIf
    RegisterForAnimationEvent(selfRef, animEventTeleportExplosionEnter)
    RegisterForAnimationEvent(selfRef, animEventTeleportExplosionExit)
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
    If selfRef == None || akSource != selfRef
        Return
    EndIf
    If asEventName == animEventTeleportExplosionEnter
        If EnterTeleportExplosion != None
            selfRef.PlaceAtMe(EnterTeleportExplosion)
        EndIf
        If EnterTeleportSound != None
            EnterTeleportSound.Play(selfRef)
        EndIf
        If TeleportChameleonSpell != None
            TeleportChameleonSpell.Cast(selfRef, selfRef)
        EndIf
    ElseIf asEventName == animEventTeleportExplosionExit
        If ExitTeleportExplosion != None
            selfRef.PlaceAtMe(ExitTeleportExplosion)
        EndIf
        If ExitTeleportSound != None
            ExitTeleportSound.Play(selfRef)
        EndIf
        If TeleportChameleonSpell != None
            selfRef.DispelSpell(TeleportChameleonSpell)
        EndIf
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If selfRef != None
        UnregisterForAnimationEvent(selfRef, animEventTeleportExplosionEnter)
        UnregisterForAnimationEvent(selfRef, animEventTeleportExplosionExit)
        If TeleportChameleonSpell != None
            selfRef.DispelSpell(TeleportChameleonSpell)
        EndIf
    EndIf
EndEvent
