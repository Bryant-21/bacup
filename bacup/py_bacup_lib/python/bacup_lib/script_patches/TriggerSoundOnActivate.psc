; Offline FO4 activation behavior for the hollow FO76 server script. Optional
; keyword/actor-value gates are permissive when unbound; the Ready/busy states
; provide the configured cooldown.

Event OnLoad()
    If bBlockActivationOnLoad
        BlockActivation(True, False)
    EndIf
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If GetState() != "Ready" || akActionRef != Game.GetPlayer()
        Return
    EndIf
    If !PlayerHasValidKeyword(akActionRef) || !PlayerHasValidActorValue(akActionRef)
        Return
    EndIf

    If HasActiveKeyword()
        BroadcastActiveSound()
    Else
        BroadcastInactiveSound()
    EndIf

    GoToState("busy")
    If fCooldownTimerLength > 0.0
        StartTimer(fCooldownTimerLength, 0)
    Else
        GoToState("Ready")
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0
        GoToState("Ready")
    EndIf
EndEvent

Function BroadcastInactiveSound()
    If InactiveSoundToTrigger != None
        InactiveSoundToTrigger.Play(Self)
    EndIf
EndFunction

Function BroadcastActiveSound()
    If ActiveSoundToTrigger != None
        ActiveSoundToTrigger.Play(Self)
    EndIf
EndFunction

Bool Function PlayerHasValidActorValue(ObjectReference akTargetRef)
    If ValidPlayerActorValue == None
        Return True
    EndIf
    Actor targetActor = akTargetRef as Actor
    Return targetActor != None && targetActor.GetValue(ValidPlayerActorValue) >= iTargetActorValue as Float
EndFunction

Bool Function PlayerHasValidKeyword(ObjectReference akTargetRef)
    Return ValidPlayerKeyword == None || akTargetRef.HasKeyword(ValidPlayerKeyword)
EndFunction

Bool Function HasActiveKeyword()
    Return MyActiveKeyword == None || HasKeyword(MyActiveKeyword)
EndFunction
