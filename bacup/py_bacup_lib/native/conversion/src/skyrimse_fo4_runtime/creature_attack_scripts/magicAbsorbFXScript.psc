Scriptname magicAbsorbFXScript Extends ActiveMagicEffect

Float fTDistance
Bool bIsFinishing = False

ImageSpaceModifier Property TrapImod Auto
VisualEffect Property TargetVFX Auto
Float Property fEffectDurationMax = 40.0 Auto
VisualEffect Property CasterVFX Auto
Float Property fImodFadeDistance = 2048.0 Auto

Event OnEffectFinish(Actor Target, Actor Caster)
    bIsFinishing = True
    If TargetVFX
        TargetVFX.Stop(Target)
    EndIf
    If CasterVFX
        CasterVFX.Stop(Caster)
    EndIf
EndEvent

Event OnEffectStart(Actor Target, Actor Caster)
    If TrapImod
        If Caster != Game.GetPlayer()
            fTDistance = Game.GetPlayer().GetDistance(Caster)
            fTDistance = fImodFadeDistance - fTDistance
            If fTDistance <= 0.0
                TrapImod.Apply(0.25)
            Else
                fTDistance = fTDistance / fImodFadeDistance
                If fTDistance < 0.25
                    fTDistance = 0.25
                EndIf
                TrapImod.Apply(fTDistance)
            EndIf
        Else
            TrapImod.Apply(1.0)
        EndIf
    EndIf
    If bIsFinishing == False
        If TargetVFX
            TargetVFX.Play(Target, fEffectDurationMax, Caster)
        EndIf
        If CasterVFX
            CasterVFX.Play(Caster, fEffectDurationMax, Target)
        EndIf
    Else
        If TargetVFX
            TargetVFX.Play(Target, 2.0, Caster)
        EndIf
        If CasterVFX
            CasterVFX.Play(Caster, 2.0, Target)
        EndIf
    EndIf
EndEvent
