Event OnEffectStart(Actor akTarget, Actor akCaster)
    selfActorRef = akTarget
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If selfActorRef && selfActorRef.IsDead()
        selfActorRef.PlaceAtMe(ExplosionOnDeath)
    EndIf
EndEvent
