; Offline FO4 nuclear-test sequence using only the button's bound link keywords
; and payloads. Linked references retain ownership of their own art/light/cloud
; behavior; this script activates them and places the bound blast at link 01.

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || GetState() == "busy"
        Return
    EndIf

    GoToState("busy")
    BroadcastKlaxonSound()

    If PlayAnim != ""
        PlayAnimation(PlayAnim)
    EndIf

    ObjectReference blastMarker = GetLinkedRef(LinkCustom01)
    If blastMarker == None
        blastMarker = Self
    EndIf

    ActivateLinkedEffect(LinkCustom01, akActionRef)
    ActivateLinkedEffect(LinkCustom02, akActionRef)
    ActivateLinkedEffect(LinkCustom03, akActionRef)

    If NukeExplosionDummy != None
        blastMarker.PlaceAtMe(NukeExplosionDummy)
    EndIf
    If EN07_Fissure_CameraShakeSpell_Intense != None
        EN07_Fissure_CameraShakeSpell_Intense.Cast(blastMarker, akActionRef)
    EndIf

    GoToState("Waiting")
EndEvent

Function ActivateLinkedEffect(Keyword linkKeyword, ObjectReference akActivator)
    ObjectReference effectRef = GetLinkedRef(linkKeyword)
    If effectRef != None
        effectRef.Activate(akActivator)
    EndIf
EndFunction

Function BroadcastKlaxonSound()
    If OBJKlaxonMineOneshot != None
        OBJKlaxonMineOneshot.Play(Self)
    EndIf
EndFunction
