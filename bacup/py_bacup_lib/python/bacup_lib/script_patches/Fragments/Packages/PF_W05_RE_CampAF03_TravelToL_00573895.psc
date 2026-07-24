Function Fragment_End(Actor akActor)
    ; The property is named Cannibal02Alias, but on this record it is bound
    ; to alias 24, which the owning quest (562281) itself declares as
    ; ClutterMarkerEnable -- the name is a stale leftover, the binding
    ; governs. Enable() matches the alias's own name and pairs with a
    ; ClutterMarkerDisable alias (31) declared elsewhere on the same quest.
    ReferenceAlias kClutterEnable = Cannibal02Alias as ReferenceAlias
    If kClutterEnable == None
        Return
    EndIf
    ObjectReference kClutterRef = kClutterEnable.GetReference()
    If kClutterRef != None
        kClutterRef.Enable()
    EndIf
EndFunction
