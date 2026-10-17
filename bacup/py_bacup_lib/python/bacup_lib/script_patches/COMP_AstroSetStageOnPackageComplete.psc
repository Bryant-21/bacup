Event OnPackageEnd(Package akOldPackage)
	Quest owner = Self.GetOwningQuest()
	If akOldPackage == TargetPackage && owner && owner.IsRunning() && !owner.IsStageDone(StageToSet)
		owner.SetStage(StageToSet)
	EndIf
EndEvent
