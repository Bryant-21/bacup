; Local-player gating becomes a player comparison (FO4 has no Actor.IsLocalPlayer).
; UnregisterForPlayerConnectionEvent is dropped: nothing registers for it in FO4.

Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget == Game.GetPlayer() && !Complete
        myImod.Apply(1.0)
        mySnapshot.Push(1.0)
        SoundID = mySound.Play(akTarget)
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    Complete = True
    If akTarget == Game.GetPlayer()
        myImod.Remove()
        playerClientTarget = akTarget
        mySnapshot.Remove()
        Sound.StopInstance(SoundID)
    EndIf
EndEvent
