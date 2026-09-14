Function DestroySelf(Bool playExplosion)
	If playExplosion
		Self.PlaceAtMe(crExplosionFlashFermenter as form, 1, False, False, True)
	EndIf
	; FO76 ObjectReference.ForceSwapModelOnClient(ref) told every client to render
	; this reference using another reference's model. Fallout 4 has no model swap, so
	; the equivalent is the standard placed-ref swap: hide this one, show the
	; pre-placed destroyed version bound to DestroyedFermenter.
	If DestroyedFermenter != None
		DestroyedFermenter.Enable(False)
	EndIf
	Self.Disable(False)
EndFunction
